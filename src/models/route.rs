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

/// 未知参数转发策略：
/// - Auto：按平台类型自动决定（Anthropic 严格校验未知字段 → 丢弃；OpenAI 兼容上游普遍忽略 → 透传）
/// - Drop：跨协议转发时丢弃未建模参数（白名单模式，Anthropic 推荐）
/// - Passthrough：原样透传（现状行为，OpenAI 类推荐）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum UnknownParams {
    Auto,
    Drop,
    Passthrough,
}

impl Default for UnknownParams {
    fn default() -> Self { UnknownParams::Auto }
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
    /// 客户端未传 max_tokens 时填充的默认值（None = 不干预；Anthropic 必填，兜底 4096）
    #[serde(default)]
    pub default_max_tokens: Option<u32>,
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
    /// 参数覆盖：JSON object，转发前 merge 进请求体（覆盖优先于客户端值）
    #[serde(default = "default_param_overrides")]
    pub param_overrides: serde_json::Value,
    /// 未知参数转发策略（默认 Auto：Anthropic 丢弃 / 其他透传）
    #[serde(default)]
    pub unknown_params: UnknownParams,
}

pub(crate) fn default_param_overrides() -> serde_json::Value { serde_json::json!({}) }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    #[serde(default = "default_lb_strategy")]
    pub lb_strategy: LBStrategy,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    pub fallback: Option<String>,
    #[serde(default)]
    pub default_max_tokens: Option<u32>,
    pub backends: Vec<CreateBackendRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRouteRequest {
    pub lb_strategy: Option<LBStrategy>,
    pub retry_policy: Option<RetryPolicy>,
    pub fallback: Option<String>,
    pub default_max_tokens: Option<u32>,
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
    #[serde(default = "default_param_overrides")]
    pub param_overrides: serde_json::Value,
    #[serde(default)]
    pub unknown_params: UnknownParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBackendRequest {
    pub weight: Option<u32>,
    pub priority: Option<u32>,
    pub max_concurrent: Option<u32>,
    pub capabilities: Option<Vec<String>>,
    pub status: Option<BackendStatus>,
    pub param_overrides: Option<serde_json::Value>,
    pub unknown_params: Option<UnknownParams>,
}

fn default_lb_strategy() -> LBStrategy { LBStrategy::RoundRobin }
fn default_weight() -> u32 { 1 }
