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
}

impl BackendSelector {
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
            connections: DashMap::new(),
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
            counter.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
