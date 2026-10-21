//! 思考（Reasoning / Extended Thinking）参数的统一模型与映射工具。
//!
//! 入站时把 OpenAI / Anthropic / 各厂商的思考字段归一化为 `ReasoningConfig`，
//! 出站时再按 backend 的 `ReasoningStyle` 生成目标平台认识的参数。

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::models::platform::PlatformType;
use crate::models::proxy::Protocol;
use crate::models::route::{ReasoningEcho, ReasoningStyle, UnknownParams};

#[derive(Debug, Clone, PartialEq)]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    None,
    Custom(String),
}

impl ReasoningEffort {
    /// 解析 effort 字符串；空串返回 None，未知值保留为 Custom。
    pub fn parse(raw: &str) -> Option<Self> {
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty() {
            return None;
        }
        Some(match value.as_str() {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" | "x-high" | "extra-high" => Self::XHigh,
            "none" | "disabled" => Self::None,
            other => Self::Custom(other.to_string()),
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::None => "none",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn enabled(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// effort → 近似 budget_tokens
    pub fn budget_tokens(&self) -> Option<u32> {
        match self {
            Self::Minimal => Some(1024),
            Self::Low => Some(2048),
            Self::Medium => Some(8192),
            Self::High => Some(16384),
            Self::XHigh => Some(32768),
            Self::None | Self::Custom(_) => None,
        }
    }

    /// budget_tokens → 近似 effort
    pub fn from_budget_tokens(budget: u32) -> Self {
        if budget >= 24576 {
            Self::XHigh
        } else if budget >= 12288 {
            Self::High
        } else if budget >= 6144 {
            Self::Medium
        } else if budget >= 2048 {
            Self::Low
        } else {
            Self::Minimal
        }
    }
}

impl Serialize for ReasoningEffort {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ReasoningEffort {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw).unwrap_or_else(|| Self::Custom(raw)))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum ReasoningSource {
    #[default]
    None,
    OpenAiEffort,
    OpenAiReasoning,
    AnthropicThinking,
    Vendor,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningConfig {
    pub enabled: Option<bool>,
    pub effort: Option<ReasoningEffort>,
    pub budget_tokens: Option<u32>,
    pub include_thoughts: Option<bool>,
    #[serde(default)]
    pub source: ReasoningSource,
    /// 客户端原始字段，同风格/同协议时优先原样回放
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub raw: Map<String, Value>,
}

impl ReasoningConfig {
    pub fn is_enabled(&self) -> bool {
        if self.enabled == Some(false) {
            return false;
        }
        if let Some(effort) = &self.effort {
            return effort.enabled();
        }
        self.budget_tokens.is_some() || self.enabled == Some(true)
    }

    /// 实际生效的 budget（显式 budget 优先，其次由 effort 映射）
    pub fn effective_budget_tokens(&self) -> Option<u32> {
        self.budget_tokens
            .or_else(|| self.effort.as_ref().and_then(|e| e.budget_tokens()))
    }

    /// 实际生效的 effort（显式 effort 优先，其次由 budget 反推）
    pub fn effective_effort(&self) -> Option<ReasoningEffort> {
        self.effort
            .clone()
            .or_else(|| self.budget_tokens.map(ReasoningEffort::from_budget_tokens))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum MaxTokensField {
    #[default]
    MaxTokens,
    MaxCompletionTokens,
}

/// 出站目标信息：用于选择思考参数风格、读取签名缓存、决定是否回传 reasoning_content。
#[derive(Debug, Clone)]
pub struct ReasoningTarget {
    pub style: ReasoningStyle,
    pub backend_id: String,
    pub model_id: String,
    pub echo: ReasoningEcho,
}

impl ReasoningTarget {
    pub fn new(
        style: ReasoningStyle,
        backend_id: impl Into<String>,
        model_id: impl Into<String>,
        echo: ReasoningEcho,
    ) -> Self {
        Self {
            style,
            backend_id: backend_id.into(),
            model_id: model_id.into(),
            echo,
        }
    }
}

/// Auto 风格按目标平台解析；显式配置保持不变。
pub fn resolve_style(style: &ReasoningStyle, platform: &PlatformType) -> ReasoningStyle {
    match style {
        ReasoningStyle::Auto => match platform {
            PlatformType::Anthropic => ReasoningStyle::AnthropicThinking,
            _ => ReasoningStyle::OpenAiEffort,
        },
        explicit => explicit.clone(),
    }
}

/// OpenAI 入口：从 extra 中取走所有思考字段。
pub fn take_openai_reasoning(extra: &mut Map<String, Value>) -> Option<ReasoningConfig> {
    let mut cfg = ReasoningConfig::default();
    let mut present = false;
    let mut explicit_disable = false;
    let mut explicit_enable = false;

    // 4. OpenRouter / OpenAI 兼容的 reasoning 对象
    if let Some(value) = extra.remove("reasoning") {
        present = true;
        cfg.raw.insert("reasoning".into(), value.clone());
        cfg.source = ReasoningSource::OpenAiReasoning;
        if let Some(obj) = value.as_object() {
            if let Some(raw) = obj.get("effort").and_then(|v| v.as_str()) {
                match ReasoningEffort::parse(raw) {
                    Some(ReasoningEffort::None) => explicit_disable = true,
                    Some(effort) => {
                        cfg.effort = Some(effort);
                        explicit_enable = true;
                    }
                    None => {}
                }
            }
            if let Some(max) = obj.get("max_tokens").and_then(|v| v.as_u64()) {
                cfg.budget_tokens = Some(max.min(u32::MAX as u64) as u32);
                explicit_enable = true;
            }
            if let Some(exclude) = obj.get("exclude").and_then(|v| v.as_bool()) {
                cfg.include_thoughts = Some(!exclude);
            }
        } else if let Some(raw) = value.as_str() {
            if let Some(effort) = ReasoningEffort::parse(raw) {
                if effort == ReasoningEffort::None {
                    explicit_disable = true;
                } else {
                    cfg.effort = Some(effort);
                    explicit_enable = true;
                }
            }
        }
    }

    // 3. 标准 OpenAI reasoning_effort（优先级高于 reasoning 对象）
    if let Some(value) = extra.remove("reasoning_effort") {
        present = true;
        cfg.raw.insert("reasoning_effort".into(), value.clone());
        cfg.source = ReasoningSource::OpenAiEffort;
        if let Some(raw) = value.as_str() {
            match ReasoningEffort::parse(raw) {
                Some(ReasoningEffort::None) => {
                    cfg.effort = None;
                    explicit_disable = true;
                }
                Some(effort) => {
                    cfg.effort = Some(effort);
                    explicit_enable = true;
                }
                None => {}
            }
        }
    }

    // 2/5. 部分 OpenAI 兼容上游的 thinking 对象
    if let Some(value) = extra.remove("thinking") {
        present = true;
        cfg.raw.insert("thinking".into(), value.clone());
        if cfg.source == ReasoningSource::None {
            cfg.source = ReasoningSource::Vendor;
        }
        if let Some(obj) = value.as_object() {
            match obj.get("type").and_then(|v| v.as_str()) {
                Some("disabled") => explicit_disable = true,
                Some("enabled") => explicit_enable = true,
                _ => {}
            }
            if let Some(budget) = obj.get("budget_tokens").and_then(|v| v.as_u64()) {
                if cfg.budget_tokens.is_none() {
                    cfg.budget_tokens = Some(budget.min(u32::MAX as u64) as u32);
                }
                explicit_enable = true;
            }
        }
    }

    // 5. 厂商布尔开关与预算
    if let Some(value) = extra.remove("enable_thinking") {
        present = true;
        cfg.raw.insert("enable_thinking".into(), value.clone());
        if cfg.source == ReasoningSource::None {
            cfg.source = ReasoningSource::Vendor;
        }
        match value.as_bool() {
            Some(true) => explicit_enable = true,
            Some(false) => explicit_disable = true,
            None => {}
        }
    }
    if let Some(value) = extra.remove("thinking_budget") {
        present = true;
        cfg.raw.insert("thinking_budget".into(), value.clone());
        if cfg.source == ReasoningSource::None {
            cfg.source = ReasoningSource::Vendor;
        }
        if let Some(budget) = value.as_u64() {
            if cfg.budget_tokens.is_none() {
                cfg.budget_tokens = Some(budget.min(u32::MAX as u64) as u32);
            }
            explicit_enable = true;
        }
    }
    if let Some(value) = extra.remove("chat_template_kwargs") {
        let mut remaining = value.as_object().cloned().unwrap_or_default();
        if let Some(enable) = remaining.remove("enable_thinking") {
            present = true;
            cfg.raw
                .insert("chat_template_kwargs".into(), value.clone());
            if cfg.source == ReasoningSource::None {
                cfg.source = ReasoningSource::Vendor;
            }
            match enable.as_bool() {
                Some(true) => explicit_enable = true,
                Some(false) => explicit_disable = true,
                None => {}
            }
        }
        if !remaining.is_empty() {
            extra.insert("chat_template_kwargs".into(), Value::Object(remaining));
        }
    }
    if let Some(value) = extra.remove("extra_body") {
        let mut remaining = value.as_object().cloned().unwrap_or_default();
        if let Some(mut google) = remaining.remove("google") {
            if let Some(thinking) = google
                .get_mut("thinking_config")
                .and_then(|v| v.as_object_mut())
            {
                present = true;
                cfg.raw.insert(
                    "google.thinking_config".into(),
                    Value::Object(thinking.clone()),
                );
                if cfg.source == ReasoningSource::None {
                    cfg.source = ReasoningSource::Vendor;
                }
                if let Some(budget) = thinking.get("thinking_budget").and_then(|v| v.as_u64()) {
                    if cfg.budget_tokens.is_none() {
                        cfg.budget_tokens = Some(budget.min(u32::MAX as u64) as u32);
                    }
                    explicit_enable = true;
                }
                if let Some(include) = thinking.get("include_thoughts").and_then(|v| v.as_bool()) {
                    cfg.include_thoughts = Some(include);
                }
                if thinking.is_empty() {
                    if let Some(obj) = google.as_object_mut() {
                        obj.remove("thinking_config");
                    }
                }
            }
            if google.as_object().map(|o| !o.is_empty()).unwrap_or(true) {
                remaining.insert("google".into(), google);
            }
        }
        if !remaining.is_empty() {
            extra.insert("extra_body".into(), Value::Object(remaining));
        }
    }

    if !present {
        return None;
    }

    if explicit_disable {
        cfg.enabled = Some(false);
        cfg.effort = None;
        cfg.budget_tokens = None;
    } else if explicit_enable {
        cfg.enabled = Some(true);
    }
    Some(cfg)
}

/// Anthropic 入口：取走顶层 thinking 对象。
pub fn take_anthropic_thinking(extra: &mut Map<String, Value>) -> Option<ReasoningConfig> {
    let value = extra.remove("thinking")?;
    let mut cfg = ReasoningConfig {
        source: ReasoningSource::AnthropicThinking,
        ..Default::default()
    };
    cfg.raw.insert("thinking".into(), value.clone());
    if let Some(obj) = value.as_object() {
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("enabled") => cfg.enabled = Some(true),
            Some("disabled") => cfg.enabled = Some(false),
            _ => {}
        }
        if let Some(budget) = obj.get("budget_tokens").and_then(|v| v.as_u64()) {
            cfg.budget_tokens = Some(budget.min(u32::MAX as u64) as u32);
            if cfg.enabled.is_none() {
                cfg.enabled = Some(true);
            }
        }
    }
    if cfg.enabled.is_none() && cfg.budget_tokens.is_none() {
        return None;
    }
    Some(cfg)
}

/// 源协议 + 目标协议 + 策略 决定某个未知字段是否透传。
pub fn should_forward_extra(
    unknown: &UnknownParams,
    source: Protocol,
    target: Protocol,
    key: &str,
) -> bool {
    match unknown {
        UnknownParams::Passthrough => true,
        UnknownParams::Drop => false,
        UnknownParams::Auto => match (source, target) {
            // Anthropic → OpenAI：剔除 Anthropic 专有字段
            (Protocol::Anthropic, Protocol::OpenAI) => {
                !matches!(
                    key,
                    "top_k"
                        | "metadata"
                        | "mcp_servers"
                        | "context_management"
                        | "container"
                        | "output_config"
                        | "service_tier"
                        | "anthropic_version"
                        | "anthropic_beta"
                        | "betas"
                )
            }
            // OpenAI → Anthropic：只保留 Anthropic 能接受的少量扩展
            (Protocol::OpenAI, Protocol::Anthropic) => matches!(
                key,
                "top_k"
                    | "metadata"
                    | "service_tier"
                    | "mcp_servers"
                    | "context_management"
                    | "container"
                    | "output_config"
                    | "betas"
            ),
            // 同协议：保持透传
            _ => true,
        },
    }
}

/// 从响应内容块里筛出 thinking / redacted_thinking，用于签名缓存。
pub fn collect_thinking_blocks(blocks: &[Value]) -> Vec<Value> {
    blocks
        .iter()
        .filter(|block| {
            matches!(
                block.get("type").and_then(|t| t.as_str()),
                Some("thinking") | Some("redacted_thinking")
            )
        })
        .cloned()
        .collect()
}

/// 是否为网关生成的合成签名（绝不能发给真实 Anthropic 上游）。
pub fn is_synthetic_signature(signature: &str) -> bool {
    signature.starts_with("gw_sig_")
}

pub fn synthetic_signature() -> String {
    format!("gw_sig_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn effort_budget_mapping() {
        assert_eq!(
            ReasoningEffort::High.budget_tokens(),
            Some(16384)
        );
        assert_eq!(
            ReasoningEffort::from_budget_tokens(10000),
            ReasoningEffort::Medium
        );
        assert_eq!(
            ReasoningEffort::from_budget_tokens(16384),
            ReasoningEffort::High
        );
        assert_eq!(
            ReasoningEffort::from_budget_tokens(30000),
            ReasoningEffort::XHigh
        );
    }

    #[test]
    fn openai_effort_is_parsed_and_removed() {
        let mut extra = json!({"reasoning_effort": "high", "seed": 1})
            .as_object()
            .unwrap()
            .clone();
        let cfg = take_openai_reasoning(&mut extra).expect("config");
        assert_eq!(cfg.effort, Some(ReasoningEffort::High));
        assert!(cfg.is_enabled());
        assert!(!extra.contains_key("reasoning_effort"));
        assert!(extra.contains_key("seed"));
    }

    #[test]
    fn explicit_disable_wins() {
        let mut extra = json!({
            "reasoning_effort": "high",
            "thinking": {"type": "disabled"}
        })
        .as_object()
        .unwrap()
        .clone();
        let cfg = take_openai_reasoning(&mut extra).expect("config");
        assert_eq!(cfg.enabled, Some(false));
        assert!(!cfg.is_enabled());
    }

    #[test]
    fn openrouter_reasoning_object() {
        let mut extra = json!({
            "reasoning": {"effort": "low", "max_tokens": 4096, "exclude": true}
        })
        .as_object()
        .unwrap()
        .clone();
        let cfg = take_openai_reasoning(&mut extra).expect("config");
        assert_eq!(cfg.effort, Some(ReasoningEffort::Low));
        assert_eq!(cfg.budget_tokens, Some(4096));
        assert_eq!(cfg.include_thoughts, Some(false));
        assert_eq!(cfg.source, ReasoningSource::OpenAiReasoning);
    }

    #[test]
    fn anthropic_thinking_object() {
        let mut extra = json!({
            "thinking": {"type": "enabled", "budget_tokens": 10000}
        })
        .as_object()
        .unwrap()
        .clone();
        let cfg = take_anthropic_thinking(&mut extra).expect("config");
        assert_eq!(cfg.enabled, Some(true));
        assert_eq!(cfg.budget_tokens, Some(10000));
        assert_eq!(cfg.source, ReasoningSource::AnthropicThinking);
    }

    #[test]
    fn auto_unknown_policy_is_source_aware() {
        assert!(should_forward_extra(
            &UnknownParams::Auto,
            Protocol::Anthropic,
            Protocol::Anthropic,
            "mcp_servers"
        ));
        assert!(!should_forward_extra(
            &UnknownParams::Auto,
            Protocol::OpenAI,
            Protocol::Anthropic,
            "response_format"
        ));
        assert!(!should_forward_extra(
            &UnknownParams::Auto,
            Protocol::Anthropic,
            Protocol::OpenAI,
            "top_k"
        ));
        assert!(should_forward_extra(
            &UnknownParams::Auto,
            Protocol::OpenAI,
            Protocol::OpenAI,
            "response_format"
        ));
    }
}
