pub mod strategy;

use crate::models::route::{Backend, BackendStatus, LBStrategy};
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 后端选择器，使用 DashMap 实现无锁并发访问
pub struct BackendSelector {
    /// round-robin 计数器：route_id → counter
    counters: DashMap<String, AtomicUsize>,
    /// 活跃连接数：(route_id:backend_id) → count
    connections: DashMap<String, AtomicUsize>,
    /// 每后端并发闸门：(route_id:backend_id) → (在途数, 上限)。
    /// 只存「配置了 max_concurrent 的后端」；上限变化时按新值重建。
    limits: DashMap<String, (AtomicUsize, usize)>,
}

impl BackendSelector {
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
            connections: DashMap::new(),
            limits: DashMap::new(),
        }
    }

    /// 尝试为某后端占用一个并发槽位。未配置 max_concurrent 的后端直接放行（返回 true）。
    /// 配置了上限且已满则返回 false，调用方应换下一个候选后端。
    pub fn try_acquire_slot(&self, route_id: &str, backend_id: &str, max_concurrent: Option<u32>) -> bool {
        let Some(limit) = max_concurrent.filter(|v| *v > 0) else {
            return true;
        };
        let limit = limit as usize;
        let key = format!("{}:{}", route_id, backend_id);
        let mut entry = self.limits.entry(key).or_insert_with(|| {
            (AtomicUsize::new(0), limit)
        });
        // 上限被改过：按新上限重建计数（在途数归零，宁可短暂放宽也不永久卡死）
        if entry.1 != limit {
            entry.1 = limit;
            entry.0.store(0, Ordering::Relaxed);
        }
        entry.0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if v < limit { Some(v + 1) } else { None }
            })
            .is_ok()
    }

    /// 释放并发槽位。未配置上限的后端是 no-op。
    pub fn release_slot(&self, route_id: &str, backend_id: &str) {
        let key = format!("{}:{}", route_id, backend_id);
        if let Some(entry) = self.limits.get(&key) {
            let _ = entry.0.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            });
        }
    }

    pub fn select<'a>(&self, route_id: &str, backends: &'a [Backend], strategy: &LBStrategy) -> Option<&'a Backend> {
        let active: Vec<&'a Backend> = backends.iter().filter(|b| b.status == BackendStatus::Active).collect();
        if active.is_empty() { return None; }

        match strategy {
            LBStrategy::RoundRobin => self.round_robin_inner(route_id, &active),
            LBStrategy::WeightedRandom => Self::weighted_random_inner(&active),
            LBStrategy::LeastConnections => self.least_connections_inner(route_id, &active),
            LBStrategy::Priority => active.iter().min_by_key(|b| b.priority).copied(),
            LBStrategy::LatencyBased => self.round_robin_inner(route_id, &active),
        }
    }

    fn round_robin_inner<'a>(&self, route_id: &str, backends: &[&'a Backend]) -> Option<&'a Backend> {
        let counter = self.counters.entry(route_id.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let idx = counter.fetch_add(1, Ordering::Relaxed) % backends.len();
        Some(backends[idx])
    }

    fn weighted_random_inner<'a>(backends: &[&'a Backend]) -> Option<&'a Backend> {
        let total_weight: u32 = backends.iter().map(|b| b.weight).sum();
        if total_weight == 0 { return backends.first().copied(); }
        let mut rng_val = rand::random::<u32>() % total_weight;
        for backend in backends {
            if rng_val < backend.weight { return Some(backend); }
            rng_val -= backend.weight;
        }
        backends.first().copied()
    }

    fn least_connections_inner<'a>(&self, route_id: &str, backends: &[&'a Backend]) -> Option<&'a Backend> {
        backends.iter().min_by_key(|b| {
            let key = format!("{}:{}", route_id, b.id);
            self.connections.get(&key).map(|c| c.load(Ordering::Relaxed)).unwrap_or(0)
        }).copied()
    }

    pub fn inc_connection(&self, route_id: &str, backend_id: &str) {
        let key = format!("{}:{}", route_id, backend_id);
        self.connections.entry(key)
            .or_insert_with(|| AtomicUsize::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_connection(&self, route_id: &str, backend_id: &str) {
        let key = format!("{}:{}", route_id, backend_id);
        if let Some(counter) = self.connections.get(&key) {
            // 饱和减法：曾经 inc 与 dec 之间的 `?` 早退会漏掉一次 dec，
            // 下一次 fetch_sub 从 0 下溢回绕成 usize::MAX，LeastConnections 会把
            // 这个后端永久避开。这里兜底不让计数低于 0。
            let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            });
        }
    }

    /// 当前全部在途连接总数（概览大屏「并发连接」KPI）
    pub fn active_connections_total(&self) -> i64 {
        self.connections
            .iter()
            .map(|entry| entry.value().load(Ordering::Relaxed) as i64)
            .sum()
    }
}
