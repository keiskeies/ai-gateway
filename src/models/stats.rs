use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStat {
    pub id: i64,
    pub proxy_id: String,
    pub route_id: String,
    pub backend_id: String,
    pub status_code: i32,
    pub latency_ms: i64,
    pub token_input: Option<i64>,
    pub token_output: Option<i64>,
    pub error_type: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStats {
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub active_proxies: i64,
    pub total_proxies: i64,
    pub active_platforms: i64,
    pub total_platforms: i64,
    pub total_models: i64,
    pub total_token_input: i64,
    pub total_token_output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStats {
    pub proxy_id: String,
    pub proxy_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub requests_today: i64,
    pub total_token_input: i64,
    pub total_token_output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStats {
    pub backend_id: String,
    pub platform_name: String,
    pub model_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub active_connections: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformStats {
    pub platform_id: String,
    pub platform_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub total_token_input: i64,
    pub total_token_output: i64,
}
