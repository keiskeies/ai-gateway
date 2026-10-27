//! 私有协议上游适配层。
//!
//! 三条路径的分工：
//! - `PlatformAdapter::OpenAI` / `Anthropic`：走现有 `protocol::openai` / `protocol::anthropic`，
//!   本模块只做转发（`build` 返回 None，调用方继续走老路径）
//! - `PlatformAdapter::Trae` / `WorkBuddy`：私有协议，请求体与鉴权头都要改写
//!
//! 设计要点：**适配器只负责「怎么构造请求」**，不负责重试、积分、统计——那些留在
//! `handler.rs`，因为它们是所有上游共用的。这样加一个新上游不需要动主链路。

pub mod error_map;
pub mod private_sse;
pub mod quota;

#[cfg(test)]
mod e2e_tests;
pub mod trae;
pub mod workbuddy;

use serde::{Deserialize, Serialize};

use crate::models::adapter::PlatformAdapter;
use crate::models::platform::Platform;
use crate::models::route::Backend;
use crate::protocol::UnifiedRequest;

/// 适配器构造出的转发计划。
#[derive(Debug)]
pub struct ForwardPlan {
    /// 完整转发地址（已含路径）
    pub url: String,
    /// 请求体（已按上游要求改写）
    pub body: serde_json::Value,
    /// 额外请求头（不含鉴权头，鉴权头由调用方统一注入）
    pub headers: Vec<(String, String)>,
    /// 是否强制流式。WorkBuddy 上游只接受 stream=true，Trae 总是回 SSE。
    pub force_stream: bool,
    /// 响应是否总是 SSE（即使请求里 stream=false）。
    /// Trae 的 llm_utils_chat 与 WorkBuddy 的 v2/chat/completions 都是这样。
    pub always_sse: bool,
}

/// 适配器需要的凭据视图。只读，不含刷新逻辑。
pub struct AdapterCredential<'a> {
    pub access_token: &'a str,
    pub refresh_token: Option<&'a str>,
    pub uid: Option<&'a str>,
    pub enterprise_id: Option<&'a str>,
    pub domain: Option<&'a str>,
    pub device_id: Option<&'a str>,
    pub machine_id: Option<&'a str>,
}

/// 构造转发计划。
///
/// 返回 `Ok(None)` 表示「标准协议，调用方走原有路径」；
/// 返回 `Ok(Some(plan))` 表示「私有协议，用这份计划转发」。
pub fn build(
    adapter: PlatformAdapter,
    platform: &Platform,
    backend: &Backend,
    unified: &UnifiedRequest,
    credential: Option<&AdapterCredential<'_>>,
    default_max_tokens: Option<u32>,
) -> crate::error::AppResult<Option<ForwardPlan>> {
    match adapter {
        PlatformAdapter::OpenAI | PlatformAdapter::Anthropic => Ok(None),
        PlatformAdapter::Trae => {
            let cred = require_credential(adapter, credential)?;
            trae::build(platform, backend, unified, cred, default_max_tokens).map(Some)
        }
        PlatformAdapter::WorkBuddy => {
            let cred = require_credential(adapter, credential)?;
            workbuddy::build(platform, backend, unified, cred, default_max_tokens).map(Some)
        }
    }
}

/// 私有协议上游必须有凭据。缺凭据时报明确错误，而不是发一个匿名请求上去。
fn require_credential<'a>(
    adapter: PlatformAdapter,
    credential: Option<&'a AdapterCredential<'a>>,
) -> crate::error::AppResult<&'a AdapterCredential<'a>> {
    credential.ok_or_else(|| {
        crate::error::AppError::BadRequest(format!(
            "{} 需要上游凭据：请先在平台详情页导入 access_token",
            adapter.label()
        ))
    })
}

/// 上游返回的业务错误（解析自 2xx 响应体或 SSE 的 error 事件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamBusinessError {
    /// 上游业务码（Trae 的 4001 / WorkBuddy 的 14018 等）。None = 纯 HTTP 错误。
    pub code: Option<i64>,
    pub message: String,
    /// 原始响应体片段，便于排查（截断到 500 字符）
    pub raw: String,
}

impl UpstreamBusinessError {
    pub fn new(code: Option<i64>, message: impl Into<String>, raw: impl Into<String>) -> Self {
        let mut raw: String = raw.into();
        if raw.chars().count() > 500 {
            raw = raw.chars().take(500).collect::<String>() + "…";
        }
        Self { code, message: message.into(), raw }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! 适配器测试共用的构造器。放在这里而不是各自复制，
    //! 是因为 `Backend` / `Platform` 每加一个字段，三份复制的字面量都要改一遍。

    use crate::models::adapter::PlatformAdapter;
    use crate::models::platform::{Platform, PlatformStatus, PlatformType};
    use crate::models::route::{Backend, BackendStatus};
    use crate::protocol::types::{Role, UnifiedMessage, UnifiedRequest};

    pub fn backend(model_id: &str) -> Backend {
        Backend {
            id: "b1".into(),
            route_id: "r1".into(),
            platform_id: "p1".into(),
            model_id: model_id.into(),
            weight: 1,
            priority: 0,
            max_concurrent: None,
            capabilities: vec![],
            param_overrides: serde_json::json!({}),
            unknown_params: Default::default(),
            reasoning_style: Default::default(),
            reasoning_echo: Default::default(),
            input_price: None,
            output_price: None,
            cache_read_price: None,
            cache_write_price: None,
            peak_pricing: None,
            credit_price: None,
            credit_multiplier: None,
            status: BackendStatus::Active,
        }
    }

    pub fn platform(base_url: &str, adapter: PlatformAdapter) -> Platform {
        Platform {
            id: "p1".into(),
            name: "test".into(),
            platform_type: PlatformType::OpenAI,
            base_url: base_url.into(),
            api_key: String::new(),
            organization: None,
            custom_headers: serde_json::json!({}),
            status: PlatformStatus::Active,
            rate_limit: None,
            credit_budget: None,
            credit_window: None,
            credit_unit: None,
            adapter: Some(format!("{adapter:?}")),
            catalog_key: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    pub fn user_message(text: &str) -> UnifiedMessage {
        UnifiedMessage {
            role: Role::User,
            content: Some(serde_json::json!(text)),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_blocks: vec![],
            raw_blocks: vec![],
        }
    }

    pub fn unified(text: &str) -> UnifiedRequest {
        UnifiedRequest {
            model: "test-model".into(),
            messages: vec![user_message(text)],
            system: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            stream: Some(true),
            stop: None,
            tools: None,
            tool_choice: None,
            source_protocol: Default::default(),
            reasoning: None,
            max_tokens_field: Default::default(),
            extra: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::platform::PlatformType;

    #[test]
    fn standard_adapters_delegate_to_legacy_path() {
        // 标准协议不该被本模块接管，否则既有的 OpenAI/Anthropic 转发会全部走空
        for adapter in [PlatformAdapter::OpenAI, PlatformAdapter::Anthropic] {
            let platform = test_support::platform("https://example.com", adapter);
            let backend = test_support::backend("test-model");
            let unified = test_support::unified("hi");
            let result = build(adapter, &platform, &backend, &unified, None, None);
            assert!(result.is_ok());
            assert!(result.unwrap().is_none(), "{:?} 不该返回 plan", adapter);
        }
    }

    #[test]
    fn proprietary_adapters_require_credential() {
        for adapter in [PlatformAdapter::Trae, PlatformAdapter::WorkBuddy] {
            let platform = test_support::platform("https://example.com", adapter);
            let backend = test_support::backend("test-model");
            let unified = test_support::unified("hi");
            let err = build(adapter, &platform, &backend, &unified, None, None);
            assert!(err.is_err(), "{:?} 缺凭据必须报错", adapter);
            let msg = format!("{}", err.unwrap_err());
            assert!(msg.contains("凭据"), "错误信息应说明缺凭据：{msg}");
        }
    }

    #[test]
    fn business_error_truncates_long_raw() {
        let long = "x".repeat(2000);
        let err = UpstreamBusinessError::new(Some(4008), "quota", long);
        assert!(err.raw.chars().count() <= 501, "raw 必须截断");
        assert!(err.raw.ends_with('…'));
    }

    #[test]
    fn platform_type_is_irrelevant_to_private_adapters() {
        // Trae/WorkBuddy 的 platform_type 仍是 OpenAI（响应协议族），
        // 但 adapter 决定请求怎么构造——两者正交
        let platform = test_support::platform("https://example.com", PlatformAdapter::Trae);
        assert_eq!(platform.platform_type, PlatformType::OpenAI);
        assert_eq!(
            PlatformAdapter::resolve(platform.adapter.as_deref(), &platform.platform_type),
            PlatformAdapter::Trae
        );
    }
}
