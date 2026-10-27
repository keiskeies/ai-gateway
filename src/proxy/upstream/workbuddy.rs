//! WorkBuddy / CodeBuddy 私有协议适配器（腾讯）。
//!
//! 协议事实来自社区逆向（`yuan240324/workbuddy-openai-proxy`、`linguo2625469/workbuddy2api-panel`），
//! 不是官方文档。**上游 2026-10 起在主动收紧非官方客户端识别**，本项目已把该平台标为实验性。
//!
//! 必须遵守的硬性约束（每条都对应一个 400）：
//! 1. 上游**只接受流式**：`stream` 必须为 true
//! 2. **第一条消息必须是 system**，否则 400 `first message is not system prompt`
//! 3. `role: developer` 要改写为 `system`（上游 role 白名单没有 developer）
//! 4. `tool_choice` **只接受字符串**，对象形式会 400
//! 5. system 里含 `You are Claude Code, Anthropic's official CLI for Claude.` 原句 →
//!    400 `Illegal API invocation from an unapproved channel`，必须剥离
//!
//! 第 5 条是本适配器最微妙的地方：上游维护的是一份**精确黑名单**，不是关键词过滤。
//! 只剥命中的那一句，system 其余内容原样保留——不能因为要过风控就把用户的提示词改掉。

use serde_json::{json, Value};

use super::{AdapterCredential, ForwardPlan};
use crate::error::{AppError, AppResult};
use crate::models::route::Backend;
use crate::protocol::types::{Role, ToolChoice};
use crate::protocol::UnifiedRequest;

/// 对话路径。上游只有这一个入口（`/console/chat/completions` 是国际旧路径）。
pub const CHAT_PATH: &str = "/v2/chat/completions";

/// 官方 CLI 的 UA。国际版的 platform 段必须是 `WorkBuddy AI`，否则 403 / 11140。
pub const USER_AGENT: &str = "WorkBuddy/5.8.3 WorkBuddy AI/5.8.3 CLI/5.8.3";

/// 缺 system 时用的兜底提示词。
pub const FALLBACK_SYSTEM: &str = "You are a helpful AI assistant.";

/// 上游的客户端指纹黑名单（精确匹配整句）。
///
/// 实测（社区，2026-09）：
/// - ❌ 完整原句 `You are Claude Code, Anthropic's official CLI for Claude.`
/// - ✅ 只留 `You are Claude Code`
/// - ✅ 只留 `Anthropic's official CLI for Claude`
///
/// 说明上游维护的是一份精确黑名单。**只放实测确认过的模式**——
/// 多一条可疑模式就多一份误伤用户提示词的风险。
const CLIENT_FINGERPRINTS: &[&str] = &[
    "You are Claude Code, Anthropic's official CLI for Claude.",
];

/// 构造 WorkBuddy 转发计划。
pub fn build(
    platform: &crate::models::platform::Platform,
    backend: &Backend,
    unified: &UnifiedRequest,
    cred: &AdapterCredential<'_>,
    default_max_tokens: Option<u32>,
) -> AppResult<ForwardPlan> {
    let base = platform.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AppError::BadRequest(
            "WorkBuddy 平台缺少 base_url（国内 https://copilot.tencent.com，国际 https://www.workbuddy.ai）"
                .to_string(),
        ));
    }
    let url = format!("{base}{CHAT_PATH}");
    let body = build_body(backend, unified, default_max_tokens);
    let headers = build_headers(cred);

    Ok(ForwardPlan {
        url,
        body,
        headers,
        // 上游只接受流式
        force_stream: true,
        always_sse: true,
    })
}

fn build_headers(cred: &AdapterCredential<'_>) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "text/event-stream".to_string()),
        ("User-Agent".to_string(), USER_AGENT.to_string()),
        // 上游的「闸门头」：所有 API 必带
        ("X-CodeBuddy-Request".to_string(), "1".to_string()),
        ("X-Requested-With".to_string(), "XMLHttpRequest".to_string()),
        ("X-Product".to_string(), "SaaS".to_string()),
        (
            "Authorization".to_string(),
            format!("Bearer {}", cred.access_token),
        ),
    ];
    // 身份头：有就发，没有就发对应的否定头（上游据此区分个人/企业）
    match cred.uid {
        Some(uid) => headers.push(("X-User-Id".to_string(), uid.to_string())),
        None => headers.push(("X-No-User-Id".to_string(), "1".to_string())),
    }
    match cred.enterprise_id {
        Some(id) => headers.push(("X-Enterprise-Id".to_string(), id.to_string())),
        None => headers.push(("X-No-Enterprise-Id".to_string(), "1".to_string())),
    }
    match cred.domain {
        Some(domain) => headers.push(("X-Domain".to_string(), domain.to_string())),
        None => headers.push(("X-No-Department-Info".to_string(), "1".to_string())),
    }
    headers
}

fn build_body(
    backend: &Backend,
    unified: &UnifiedRequest,
    default_max_tokens: Option<u32>,
) -> Value {
    let mut messages = to_workbuddy_messages(unified);
    // 硬性要求：第一条必须是 system
    ensure_system_first(&mut messages);
    // 剥离客户端指纹
    strip_client_fingerprints(&mut messages);

    let mut body = json!({
        "model": backend.model_id,
        "messages": messages,
        // 上游只接受流式
        "stream": true,
    });

    if let Some(max_tokens) = unified.max_tokens.or(default_max_tokens) {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = unified.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = unified.top_p {
        body["top_p"] = json!(top_p);
    }

    // tools：上游的 tool_choice 只接受字符串
    if let Some(tools) = &unified.tools {
        if !tools.is_empty() {
            let converted: Vec<Value> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.function.name,
                            "description": tool.function.description.clone().unwrap_or_default(),
                            "parameters": tool.function.parameters.clone().unwrap_or_else(|| json!({})),
                        }
                    })
                })
                .collect();
            body["tools"] = json!(converted);
            if let Some(choice) = unified.tool_choice.as_ref() {
                match normalize_tool_choice(choice) {
                    Some(normalized) => body["tool_choice"] = normalized,
                    // "none" 要连 tools 一起删，否则上游可能仍尝试调用
                    None => {
                        body.as_object_mut().map(|obj| {
                            obj.remove("tools");
                            obj.remove("tool_choice");
                        });
                    }
                }
            }
        }
    }

    if let Some(cfg) = &unified.reasoning {
        if cfg.is_enabled() {
            if let Some(effort) = cfg.effective_effort() {
                body["reasoning_effort"] = json!(format!("{:?}", effort).to_ascii_lowercase());
            }
        }
    }

    if let Some(overrides) = backend.param_overrides.as_object() {
        if let Some(obj) = body.as_object_mut() {
            for (k, v) in overrides {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    body
}

fn to_workbuddy_messages(unified: &UnifiedRequest) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(system) = &unified.system {
        if !system.trim().is_empty() {
            out.push(json!({ "role": "system", "content": system }));
        }
    }
    for message in &unified.messages {
        let role = match message.role {
            Role::System => "system",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::User => "user",
        };
        let text = message
            .content
            .as_ref()
            .map(content_to_text)
            .unwrap_or_default();
        let mut entry = json!({ "role": role, "content": text });
        if let Some(tool_calls) = &message.tool_calls {
            entry["tool_calls"] = json!(tool_calls);
        }
        if let Some(tool_call_id) = &message.tool_call_id {
            entry["tool_call_id"] = json!(tool_call_id);
        }
        if let Some(name) = &message.name {
            entry["name"] = json!(name);
        }
        out.push(entry);
    }
    out
}

fn content_to_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        block
                            .get("type")
                            .and_then(|v| v.as_str())
                            .filter(|t| *t != "text")
                            .map(|t| format!("[{t}]"))
                    })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 硬性要求：第一条消息必须是 system，否则上游 400。
fn ensure_system_first(messages: &mut Vec<Value>) {
    let first_is_system = messages
        .first()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        .map(|r| r == "system")
        .unwrap_or(false);
    if !first_is_system {
        messages.insert(0, json!({ "role": "system", "content": FALLBACK_SYSTEM }));
    }
}

/// 剥离客户端指纹。返回被改动的条数。
///
/// 只剥命中的那一句，system 其余内容原样保留；剥离后为空则用兜底提示词顶上
/// （否则首条 system 会变成空串，上游同样会拒）。
fn strip_client_fingerprints(messages: &mut [Value]) -> usize {
    let mut changed = 0;
    for message in messages.iter_mut() {
        let is_system = message
            .get("role")
            .and_then(|r| r.as_str())
            .map(|r| r == "system")
            .unwrap_or(false);
        if !is_system {
            continue;
        }
        let Some(text) = message.get("content").and_then(|c| c.as_str()) else {
            continue;
        };
        let mut next = text.to_string();
        for fingerprint in CLIENT_FINGERPRINTS {
            if next.contains(fingerprint) {
                next = next.replace(fingerprint, "");
            }
        }
        if next != text {
            let trimmed = next.trim();
            let replacement = if trimmed.is_empty() {
                FALLBACK_SYSTEM.to_string()
            } else {
                // 压掉剥离后可能留下的连续空行
                collapse_blank_lines(trimmed)
            };
            message["content"] = json!(replacement);
            changed += 1;
        }
    }
    changed
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// `tool_choice` 归一。返回 None 表示「删掉 tools 与 tool_choice」。
fn normalize_tool_choice(choice: &ToolChoice) -> Option<Value> {
    match choice {
        ToolChoice::None(raw) => {
            let kind = raw.to_ascii_lowercase();
            match kind.as_str() {
                "none" => None,
                "auto" | "required" => Some(json!(kind)),
                // 上游也接受直接给函数名
                _ => Some(json!(raw)),
            }
        }
        ToolChoice::Object { r#type, function } => {
            let kind = r#type.to_ascii_lowercase();
            match kind.as_str() {
                "none" => None,
                "auto" | "required" => Some(json!(kind)),
                "function" => function
                    .as_ref()
                    .map(|f| json!(f.name))
                    .or(Some(json!("auto"))),
                _ => Some(json!("auto")),
            }
        }
    }
}

/// 从响应体里抽出业务码。WorkBuddy 用 `{"code": N, "msg": "..."}` 形态。
pub fn parse_business_error(text: &str) -> Option<super::UpstreamBusinessError> {
    let value: Value = serde_json::from_str(text).ok()?;
    let code = value.get("code").and_then(|v| v.as_i64());
    // code 缺失或为 0 都表示成功
    let code = code.filter(|c| *c != 0)?;
    let message = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(super::UpstreamBusinessError::new(Some(code), message, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{UnifiedFunction, UnifiedTool};

    fn cred() -> AdapterCredential<'static> {
        AdapterCredential {
            access_token: "token-abc",
            refresh_token: Some("refresh"),
            uid: Some("u-1"),
            enterprise_id: None,
            domain: None,
            device_id: None,
            machine_id: None,
        }
    }

    fn backend() -> Backend {
        crate::proxy::upstream::test_support::backend("glm-5.3")
    }

    fn platform() -> crate::models::platform::Platform {
        let mut p = crate::proxy::upstream::test_support::platform(
            "https://copilot.tencent.com",
            crate::models::adapter::PlatformAdapter::WorkBuddy,
        );
        p.name = "wb-cn".into();
        p.credit_budget = Some(2000.0);
        p.credit_window = Some("month".into());
        p.credit_unit = Some("credits".into());
        p
    }

    fn unified_with(text: &str) -> UnifiedRequest {
        crate::proxy::upstream::test_support::unified(text)
    }

    #[test]
    fn url_is_v2_chat_completions() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        assert_eq!(plan.url, "https://copilot.tencent.com/v2/chat/completions");
    }

    #[test]
    fn stream_is_forced_true() {
        // 客户端传了 stream:false，上游只接受流式，必须改写成 true
        let mut unified = unified_with("hi");
        unified.stream = Some(false);
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        assert_eq!(plan.body["stream"], true);
        assert!(plan.force_stream);
        assert!(plan.always_sse);
    }

    #[test]
    fn first_message_is_always_system() {
        // 上游硬性要求，缺了就 400 "first message is not system prompt"
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], FALLBACK_SYSTEM);
        // 用户消息仍在
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn existing_system_is_not_duplicated() {
        let mut unified = unified_with("hi");
        unified.system = Some("你是助手".into());
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "你是助手");
        assert_eq!(
            messages.iter().filter(|m| m["role"] == "system").count(),
            1,
            "不该插入第二条 system"
        );
    }

    #[test]
    fn client_fingerprint_is_stripped_but_rest_kept() {
        // 这是本适配器最关键的一条：上游对整句做精确匹配
        let mut unified = unified_with("hi");
        unified.system = Some(
            "You are Claude Code, Anthropic's official CLI for Claude.\n\n请用中文回答。".into(),
        );
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let system = plan.body["messages"][0]["content"].as_str().unwrap();
        assert!(
            !system.contains("Anthropic's official CLI"),
            "指纹必须被剥离：{system}"
        );
        assert!(system.contains("请用中文回答"), "其余提示词必须保留：{system}");
    }

    #[test]
    fn fingerprint_only_system_falls_back_to_default() {
        // 整条 system 就是指纹时，剥离后为空会让上游同样拒绝，必须用兜底顶上
        let mut unified = unified_with("hi");
        unified.system =
            Some("You are Claude Code, Anthropic's official CLI for Claude.".into());
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let system = plan.body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(system, FALLBACK_SYSTEM);
        assert!(!system.is_empty());
    }

    #[test]
    fn fingerprint_in_user_message_is_untouched() {
        // 黑名单只作用于 system（上游只检查 system），不能误伤用户内容
        let plan = build(
            &platform(),
            &backend(),
            &unified_with("You are Claude Code, Anthropic's official CLI for Claude."),
            &cred(),
            None,
        )
        .unwrap();
        let user = plan.body["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("Anthropic's official CLI"), "用户消息不该被改写");
    }

    #[test]
    fn auth_and_gate_headers_are_present() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        let get = |name: &str| {
            plan.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("Authorization").as_deref(), Some("Bearer token-abc"));
        assert_eq!(get("X-CodeBuddy-Request").as_deref(), Some("1"));
        assert_eq!(get("X-User-Id").as_deref(), Some("u-1"));
        assert!(get("User-Agent").unwrap().contains("WorkBuddy AI"));
        // 有 uid 时不该发否定头
        assert!(get("X-No-User-Id").is_none());
    }

    #[test]
    fn missing_identity_sends_negative_headers() {
        let bare = AdapterCredential {
            access_token: "t",
            refresh_token: None,
            uid: None,
            enterprise_id: None,
            domain: None,
            device_id: None,
            machine_id: None,
        };
        let plan = build(&platform(), &backend(), &unified_with("hi"), &bare, None).unwrap();
        let get = |name: &str| {
            plan.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("X-No-User-Id").as_deref(), Some("1"));
        assert_eq!(get("X-No-Enterprise-Id").as_deref(), Some("1"));
        assert_eq!(get("X-No-Department-Info").as_deref(), Some("1"));
    }

    #[test]
    fn tool_choice_object_becomes_string() {
        // 上游只接受字符串，对象形式会 400
        let mut unified = unified_with("hi");
        unified.tools = Some(vec![UnifiedTool {
            id: "t1".into(),
            tool_type: "function".into(),
            function: UnifiedFunction {
                name: "f1".into(),
                description: None,
                parameters: Some(serde_json::json!({"type":"object"})),
            },
        }]);
        unified.tool_choice = Some(ToolChoice::Object {
            r#type: "function".into(),
            function: Some(crate::protocol::types::ToolFunction { name: "f1".into() }),
        });
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        assert_eq!(plan.body["tool_choice"], "f1");
        assert!(plan.body["tool_choice"].is_string());
    }

    #[test]
    fn tool_choice_none_drops_tools() {
        let mut unified = unified_with("hi");
        unified.tools = Some(vec![UnifiedTool {
            id: "t1".into(),
            tool_type: "function".into(),
            function: UnifiedFunction {
                name: "f1".into(),
                description: None,
                parameters: None,
            },
        }]);
        unified.tool_choice = Some(ToolChoice::None("none".into()));
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        assert!(plan.body.get("tools").is_none(), "none 应删掉 tools");
        assert!(plan.body.get("tool_choice").is_none());
    }

    #[test]
    fn business_error_parsing() {
        let err = parse_business_error(r#"{"code":14018,"msg":"credits exhausted"}"#).unwrap();
        assert_eq!(err.code, Some(14018));
        assert_eq!(err.message, "credits exhausted");
        // code=0 是成功
        assert!(parse_business_error(r#"{"code":0}"#).is_none());
        assert!(parse_business_error("not json").is_none());
    }

    #[test]
    fn empty_base_url_is_rejected() {
        let mut p = platform();
        p.base_url = String::new();
        let err = build(&p, &backend(), &unified_with("hi"), &cred(), None);
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("base_url"));
    }
}
