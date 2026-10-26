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

/// 出站思考参数适配风格。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ReasoningStyle {
    /// 按平台自动：Anthropic → AnthropicThinking，其他 → OpenAiEffort
    Auto,
    /// OpenAI 标准 reasoning_effort
    OpenAiEffort,
    /// Anthropic thinking.budget_tokens
    AnthropicThinking,
    /// OpenRouter reasoning 对象
    OpenRouter,
    /// Qwen / SiliconFlow：enable_thinking + thinking_budget
    Qwen,
    /// GLM：thinking.type
    Glm,
    /// vLLM + Qwen 模板：chat_template_kwargs.enable_thinking
    VllmQwen,
    /// 忽略客户端思考参数，不向上游发送任何思考字段
    Disabled,
}

impl Default for ReasoningStyle {
    fn default() -> Self { ReasoningStyle::Auto }
}

/// assistant 历史中的 reasoning_content 是否回传（仅 OpenAI 目标生效）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ReasoningEcho {
    /// 默认：不回传（DeepSeek Reasoner 等要求）
    Drop,
    /// 原样回传（部分 Kimi/GLM 多轮工具调用需要）
    Keep,
    /// 预留：按平台/模型判断，当前等价 Drop
    Auto,
}

impl Default for ReasoningEcho {
    fn default() -> Self { ReasoningEcho::Drop }
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
    /// 未知参数转发策略（默认 Auto：按源/目标协议自动决定）
    #[serde(default)]
    pub unknown_params: UnknownParams,
    /// 出站思考参数适配风格
    #[serde(default)]
    pub reasoning_style: ReasoningStyle,
    /// assistant reasoning_content 回传策略
    #[serde(default)]
    pub reasoning_echo: ReasoningEcho,
    /// 输入单价（每 100 万 token）：读取路径按 platform_id + model_id 关联 models 表补齐；
    /// 模型行缺失或未定价为 None
    #[serde(default)]
    pub input_price: Option<f64>,
    /// 输出单价（每 100 万 token）；口径同 input_price
    #[serde(default)]
    pub output_price: Option<f64>,
    /// 缓存命中输入价（每 100 万 token）；NULL → 回退 input_price
    #[serde(default)]
    pub cache_read_price: Option<f64>,
    /// 缓存写入输入价（每 100 万 token）；NULL → 回退 input_price
    #[serde(default)]
    pub cache_write_price: Option<f64>,
    /// 高峰时段价格覆盖；NULL → 始终基础价
    #[serde(default)]
    pub peak_pricing: Option<serde_json::Value>,
    /// 每次请求消耗的上游积分；口径同 models.credit_price
    #[serde(default)]
    pub credit_price: Option<f64>,
}

impl Backend {
    /// 单价对（每 100 万 token）。入价或出价任一侧缺失就返回 None——半截价格算不出成本，
    /// 与其猜一个方向，不如整单不估算（成本列为 NULL，聚合也不把它算进覆盖率）。
    pub fn price_pair(&self) -> Option<(f64, f64)> {
        match (self.input_price, self.output_price) {
            (Some(input), Some(output)) => Some((input, output)),
            _ => None,
        }
    }

    /// 本条请求的成本估算：单价齐备且上游回了两侧 token 数才算得出，否则 None。
    /// token 数为 0 时结果就是 0（与该行自己记录的用量保持一致）。
    pub fn estimate_cost(&self, token_input: Option<i64>, token_output: Option<i64>) -> Option<f64> {
        cost_from_prices(self.price_pair(), token_input, token_output)
    }

    /// 热路径的价格快照：基础价（缓存价缺失回退输入价）+ 解析后的高峰覆盖。
    pub fn pricing_snapshot(&self) -> Option<crate::models::pricing::Pricing> {
        let input = self.input_price?;
        let output = self.output_price?;
        let peak = self
            .peak_pricing
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        Some(crate::models::pricing::Pricing {
            input,
            output,
            cache_read: self.cache_read_price.unwrap_or(input),
            cache_write: self.cache_write_price.unwrap_or(input),
            peak,
        })
    }

    /// 按缓存命中/写入 token 与请求时刻计费；缓存 token 缺失按 0。
    pub fn estimate_cost_detailed(
        &self,
        token_input: Option<i64>,
        token_output: Option<i64>,
        cache_read: Option<i64>,
        cache_write: Option<i64>,
        at: chrono::DateTime<chrono::Local>,
    ) -> Option<f64> {
        self.pricing_snapshot()?
            .cost(token_input, token_output, cache_read, cache_write, at)
    }
}

/// 成本公式：`(输入 token × 入价 + 输出 token × 出价) / 100 万`。
/// 单价或 token 任一不齐 → None。缓存命中/写入 token 没有独立定价列，按普通输入价计入
/// （Anthropic 的 input_tokens 不含缓存部分，因此重缓存流量会低估）。
pub(crate) fn cost_from_prices(
    prices: Option<(f64, f64)>,
    token_input: Option<i64>,
    token_output: Option<i64>,
) -> Option<f64> {
    let (input_price, output_price) = prices?;
    let input = token_input? as f64;
    let output = token_output? as f64;
    Some((input * input_price + output * output_price) / 1_000_000.0)
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
    #[serde(default)]
    pub reasoning_style: ReasoningStyle,
    #[serde(default)]
    pub reasoning_echo: ReasoningEcho,
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
    pub reasoning_style: Option<ReasoningStyle>,
    pub reasoning_echo: Option<ReasoningEcho>,
}

fn default_lb_strategy() -> LBStrategy { LBStrategy::RoundRobin }
fn default_weight() -> u32 { 1 }

#[cfg(test)]
mod tests {
    use super::*;

    fn priced(input: Option<f64>, output: Option<f64>) -> Backend {
        Backend {
            id: "b1".into(), route_id: "r1".into(), platform_id: "p1".into(), model_id: "m1".into(),
            weight: 1, priority: 0, max_concurrent: None, capabilities: vec![],
            status: BackendStatus::Active,
            param_overrides: default_param_overrides(),
            unknown_params: UnknownParams::Auto,
            reasoning_style: ReasoningStyle::Auto,
            reasoning_echo: ReasoningEcho::default(),
            input_price: input,
            output_price: output,
            cache_read_price: None,
            cache_write_price: None,
            peak_pricing: None,
            credit_price: None,
        }
    }

    #[test]
    fn cost_divides_per_million_token_prices() {
        // 入价 2.5 / 出价 4.0（每 100 万 token）：1000 入 + 500 出 = (2500 + 2000) / 1e6
        let b = priced(Some(2.5), Some(4.0));
        let cost = b.estimate_cost(Some(1000), Some(500)).expect("两侧齐备应算出成本");
        assert!((cost - 0.0045).abs() < 1e-12, "实际 {cost}");
        // 零用量就是零成本：与该行自己记录的 token 保持一致
        assert_eq!(b.estimate_cost(Some(0), Some(0)), Some(0.0));
    }

    #[test]
    fn pricing_snapshot_carries_cache_prices_and_peak_tier() {
        use chrono::TimeZone;
        let mut b = priced(Some(2.0), Some(8.0));
        b.cache_read_price = Some(0.5);
        b.peak_pricing = Some(serde_json::json!({
            "windows": [{"start": "08:30", "end": "17:30"}],
            "peak": {"input": 3.0, "cache_read": 0.6}
        }));
        let pricing = b.pricing_snapshot().expect("双侧定价");
        let peak_at = chrono::Local.with_ymd_and_hms(2026, 10, 26, 10, 0, 0).unwrap();
        let off_at = chrono::Local.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap();
        // 100 万命中 token：高峰省 3.0−0.6，空闲省 2.0−0.5
        assert!((pricing.cache_savings(Some(1_000_000), peak_at).unwrap() - 2.4).abs() < 1e-12);
        assert!((pricing.cache_savings(Some(1_000_000), off_at).unwrap() - 1.5).abs() < 1e-12);
        // 缓存命中价缺失时回退输入价 → 价差为 0，而不是 None（有定价、但没省到）
        b.cache_read_price = None;
        let flat = b.pricing_snapshot().unwrap();
        assert_eq!(flat.cache_savings(Some(1_000_000), off_at), Some(0.0));
    }

    #[test]
    fn half_priced_model_is_not_estimated() {
        // 只填一侧时整单不估算（宁可不给数，也不按 0 价算出个偏低的车身价）
        assert_eq!(priced(Some(2.5), None).price_pair(), None);
        assert_eq!(priced(None, Some(4.0)).price_pair(), None);
        assert_eq!(priced(Some(2.5), None).estimate_cost(Some(1000), Some(500)), None);
        assert_eq!(priced(None, None).estimate_cost(Some(1000), Some(500)), None);
    }

    #[test]
    fn missing_usage_is_not_estimated() {
        let b = priced(Some(2.5), Some(4.0));
        assert_eq!(b.estimate_cost(None, Some(500)), None);
        assert_eq!(b.estimate_cost(Some(1000), None), None);
        assert_eq!(b.estimate_cost(None, None), None);
    }
}
