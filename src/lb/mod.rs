pub mod strategy;

use crate::models::route::{Backend, BackendStatus, LBStrategy};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use once_cell::sync;

/// 后端选择器，维护每个路由的后端状态
pub struct BackendSelector {
    counters: sync::Lazy<std::sync::Mutex<HashMap<String, usize>>>,
    connections: sync::Lazy<std::sync::Mutex<HashMap<String, usize>>>,
}

impl BackendSelector {
    pub fn new() -> Self {
        Self {
            counters: sync::Lazy::new(Default::default),
            connections: sync::Lazy::new(Default::default),
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
        let mut counters = self.counters.lock().unwrap();
        let counter = counters.entry(route_id.to_string()).or_insert(0);
        let idx = *counter % backends.len();
        *counter += 1;
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
        let conns = self.connections.lock().unwrap();
        backends.iter().min_by_key(|b| {
            let key = format!("{}:{}", route_id, b.id);
            *conns.get(&key).unwrap_or(&0)
        }).copied()
    }

    pub fn inc_connection(&self, route_id: &str, backend_id: &str) {
        let key = format!("{}:{}", route_id, backend_id);
        let mut conns = self.connections.lock().unwrap();
        *conns.entry(key).or_insert(0) += 1;
    }

    pub fn dec_connection(&self, route_id: &str, backend_id: &str) {
        let key = format!("{}:{}", route_id, backend_id);
        let mut conns = self.connections.lock().unwrap();
        if let Some(count) = conns.get_mut(&key) {
            if *count > 0 { *count -= 1; }
        }
    }
}
