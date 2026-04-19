use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum LBStrategy {
    RoundRobin,
    WeightedRandom,
    LeastConnections,
    Priority,
    LatencyBased,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum BackendStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ErrorType {
    RateLimit,
    ServerError,
    Timeout,
    ConnectionError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub retry_on_error: Vec<ErrorType>,
    pub backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            retry_on_error: vec![ErrorType::RateLimit, ErrorType::ServerError, ErrorType::Timeout],
            backoff_ms: 500,
        }
    }
}

/// Route links a proxy (virtual model) to its backend models.
/// Each proxy has exactly one route. The virtual_model name comes from the proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub proxy_id: String,
    pub backends: Vec<Backend>,
    pub lb_strategy: LBStrategy,
    pub retry_policy: RetryPolicy,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    pub id: String,
    pub route_id: String,
    pub platform_id: String,
    pub model_id: String,
    pub weight: u32,
    pub priority: u32,
    pub max_concurrent: Option<u32>,
    pub capabilities: Vec<String>,
    pub status: BackendStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    #[serde(default = "default_lb_strategy")]
    pub lb_strategy: LBStrategy,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    pub fallback: Option<String>,
    pub backends: Vec<CreateBackendRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRouteRequest {
    pub lb_strategy: Option<LBStrategy>,
    pub retry_policy: Option<RetryPolicy>,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackendRequest {
    pub platform_id: String,
    pub model_id: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub priority: u32,
    pub max_concurrent: Option<u32>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBackendRequest {
    pub weight: Option<u32>,
    pub priority: Option<u32>,
    pub max_concurrent: Option<u32>,
    pub capabilities: Option<Vec<String>>,
    pub status: Option<BackendStatus>,
}

fn default_lb_strategy() -> LBStrategy { LBStrategy::RoundRobin }
fn default_weight() -> u32 { 1 }
