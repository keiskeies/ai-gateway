//! Trae 私有协议适配器（国内 `trae.cn` / 海外 `trae.ai`）。
//!
//! 协议事实来自社区实测（`wangqi233/trae2api` 的 `docs/PROTOCOL.md`、
//! `autumnsentiment/Trae2api-cn`），不是官方文档。上游客户端升级可能失效。
//!
//! 几个必须遵守的硬性约束（每条都对应一个 400）：
//! 1. `Authorization: Cloud-IDE-JWT <token>`（不是 Bearer）
//! 2. 所有 message 的 `content` 必须是**数组**形态，字符串会 400
//! 3. `tools[].function.parameters` 必须是 **JSON 字符串**，传 object 会 400
//! 4. 上游发 tool_calls 时 `finish_reason` 仍是 `"stop"`，必须改写为 `"tool_calls"`
//! 5. `llm_utils_chat` 无论 `stream:false` 与否都回 SSE，解析一律按 SSE
//!
//! 本适配器**不伪造**设备指纹：`x-device-id` / `x-machine-id` 来自用户导入的凭据
//! （即他自己客户端的真实值）。缺失时如实不发，让上游拒绝，而不是生成假值。

use serde_json::{json, Value};

use super::{AdapterCredential, ForwardPlan};
use crate::error::{AppError, AppResult};
use crate::models::route::Backend;
use crate::protocol::types::{Role, ToolChoice};
use crate::protocol::UnifiedRequest;

/// 默认对话通道。
///
/// `solo_work_lite` 排队较轻；`chat_v3` 是通用池，热门模型常排 1000+ 位。
/// 第一版只用前者，因为后者容易把「排队久」误判成「上游故障」。
pub const DEFAULT_FUNCTION: &str = "solo_work_lite";

/// 对话路径。Trae 的模型数据面在 `trae-api-cn.mchost.guru` / `core-normal.traeapi.us`。
pub const CHAT_PATH: &str = "/api/agent/v3/llm_utils_chat";

/// Trae 的 IDE 版本头。社区实测值；上游按它决定可用模型目录。
pub const IDE_VERSION: &str = "3.5.91";
pub const IDE_VERSION_CODE: &str = "20250917";

/// 构造 Trae 转发计划。
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
            "Trae 平台缺少 base_url（国内 https://trae-api-cn.mchost.guru，海外 https://core-normal.traeapi.us）"
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
        // Trae 的 llm_utils_chat 总是回 SSE；请求里的 stream 字段不影响响应形态
        force_stream: true,
        always_sse: true,
    })
}

/// 请求头。
///
/// 设备标识只在凭据里有时才发——这是「如实携带」与「伪造」的分界线：
/// 用户从自己客户端导出的凭据里带什么，就发什么。
fn build_headers(cred: &AdapterCredential<'_>) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "text/event-stream".to_string()),
        ("x-ide-version".to_string(), IDE_VERSION.to_string()),
        ("x-ide-version-code".to_string(), IDE_VERSION_CODE.to_string()),
        // Trae 鉴权是 Cloud-IDE-JWT 前缀，不是 Bearer
        (
            "Authorization".to_string(),
            format!("Cloud-IDE-JWT {}", cred.access_token),
        ),
    ];
    if let Some(device_id) = cred.device_id {
        headers.push(("x-device-id".to_string(), device_id.to_string()));
    }
    if let Some(machine_id) = cred.machine_id {
        headers.push(("x-machine-id".to_string(), machine_id.to_string()));
    }
    headers
}

/// 请求体。
fn build_body(
    backend: &Backend,
    unified: &UnifiedRequest,
    default_max_tokens: Option<u32>,
) -> Value {
    let messages = to_trae_messages(unified);
    let mut body = json!({
        // config_name 必须是当前 function 目录里的名字，否则 4001
        "config_name": backend.model_id,
        "function": DEFAULT_FUNCTION,
        "messages": messages,
        // 上游只接受流式；即便传 false 也会回 SSE，不如显式声明
        "stream": true,
    });

    if let Some(max_tokens) = unified
        .max_tokens
        .or(default_max_tokens)
        .or(Some(4096))
    {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = unified.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = unified.top_p {
        body["top_p"] = json!(top_p);
    }

    // tools：parameters 必须是 JSON 字符串（传 object 上游 400）
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
                            // 关键：序列化成字符串，不是嵌套对象
                            "parameters": serde_json::to_string(
                                tool.function.parameters.as_ref().unwrap_or(&json!({}))
                            )
                            .unwrap_or_else(|_| "{}".to_string()),
                        }
                    })
                })
                .collect();
            body["tools"] = json!(converted);
            if let Some(choice) = unified.tool_choice.as_ref() {
                body["tool_choice"] = normalize_tool_choice(choice);
            }
        }
    }

    // 推理深度：Trae 原生档位是 light / high / extra_high
    if let Some(cfg) = &unified.reasoning {
        if cfg.is_enabled() {
            body["reasoning_effort"] = json!(map_reasoning_effort(cfg));
        }
    }

    // backend 级参数覆盖最后合并（与标准路径同口径）
    if let Some(overrides) = backend.param_overrides.as_object() {
        if let Some(obj) = body.as_object_mut() {
            for (k, v) in overrides {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    body
}

/// 把统一消息转成 Trae 形态：`content` 一律是数组。
fn to_trae_messages(unified: &UnifiedRequest) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(system) = &unified.system {
        if !system.trim().is_empty() {
            out.push(json!({
                "role": "system",
                "content": [{ "type": "text", "text": system }]
            }));
        }
    }
    for message in &unified.messages {
        let role = match message.role {
            Role::System => "system",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::User => "user",
        };
        // 关键：content 必须是数组。字符串形态会被上游判成
        // 4001 "cannot unmarshal string into ...LLMRawMessageContent"。
        // 统一消息的 content 可能是字符串、内容块数组或 None，统一走 content_to_text 再包一层。
        let text = message
            .content
            .as_ref()
            .map(content_to_text)
            .unwrap_or_default();
        let mut entry = json!({
            "role": role,
            "content": [{ "type": "text", "text": text }]
        });
        if let Some(name) = &message.name {
            entry["name"] = json!(name);
        }
        if let Some(tool_calls) = &message.tool_calls {
            entry["tool_calls"] = json!(tool_calls);
        }
        if let Some(tool_call_id) = &message.tool_call_id {
            entry["tool_call_id"] = json!(tool_call_id);
        }
        out.push(entry);
    }
    out
}

/// 把统一 content（字符串 / 内容块数组 / null）拍平成纯文本。
///
/// Trae 走的是纯文本通道，图片块不在第一版支持范围内——遇到非文本块时
/// 保留其文字描述而不是丢弃整条消息，避免多模态请求变成空消息。
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
                    // 非文本块（图片等）退化成占位，保住消息结构
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

/// `tool_choice` 归一。上游只接受字符串或对象，`none` 要连 tools 一起删（由调用方处理）。
fn normalize_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::None(raw) => json!(raw),
        ToolChoice::Object { r#type, function } => {
            let kind = r#type.to_ascii_lowercase();
            match kind.as_str() {
                "auto" | "required" | "none" => json!(kind),
                "function" => function
                    .as_ref()
                    .map(|f| json!(f.name))
                    .unwrap_or_else(|| json!("auto")),
                _ => json!("auto"),
            }
        }
    }
}

/// 统一推理档位 → Trae 原生三档。
fn map_reasoning_effort(cfg: &crate::protocol::reasoning::ReasoningConfig) -> &'static str {
    use crate::protocol::reasoning::ReasoningEffort;
    match cfg.effective_effort() {
        Some(ReasoningEffort::Low) | Some(ReasoningEffort::Minimal) => "light",
        Some(ReasoningEffort::Medium) => "high",
        Some(ReasoningEffort::High) => "extra_high",
        // 上游没有 "auto" 这个值，缺省走中间档
        _ => "high",
    }
}

/// 从 SSE 的 `data:` 行里抽出业务错误码。Trae 把错误放在 200 + SSE error 事件里。
pub fn parse_business_error(code_text: &str) -> Option<super::UpstreamBusinessError> {
    let value: Value = serde_json::from_str(code_text).ok()?;
    let code = value
        .get("code")
        .and_then(|v| v.as_i64())
        .or_else(|| value.get("error").and_then(|e| e.get("code")).and_then(|v| v.as_i64()))?;
    if code == 0 {
        return None;
    }
    let message = value
        .get("message")
        .or_else(|| value.get("msg"))
        .or_else(|| value.get("error").and_then(|e| e.get("message")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(super::UpstreamBusinessError::new(
        Some(code),
        message,
        code_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{UnifiedFunction, UnifiedMessage, UnifiedTool};

    fn cred() -> AdapterCredential<'static> {
        AdapterCredential {
            access_token: "jwt-abc",
            refresh_token: None,
            uid: None,
            enterprise_id: None,
            domain: None,
            device_id: None,
            machine_id: None,
        }
    }

    fn backend() -> Backend {
        crate::proxy::upstream::test_support::backend("glm-5.2")
    }

    fn platform() -> crate::models::platform::Platform {
        let mut p = crate::proxy::upstream::test_support::platform(
            "https://trae-api-cn.mchost.guru/",
            crate::models::adapter::PlatformAdapter::Trae,
        );
        p.name = "trae-cn".into();
        p
    }

    fn unified_with(text: &str) -> UnifiedRequest {
        crate::proxy::upstream::test_support::unified(text)
    }

    #[test]
    fn url_is_joined_without_double_slash() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        assert_eq!(
            plan.url,
            "https://trae-api-cn.mchost.guru/api/agent/v3/llm_utils_chat"
        );
    }

    #[test]
    fn auth_header_uses_cloud_ide_jwt_prefix() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        let auth = plan
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.clone())
            .expect("必须带 Authorization");
        assert!(auth.starts_with("Cloud-IDE-JWT "), "实际：{auth}");
        assert!(!auth.starts_with("Bearer"), "Trae 不用 Bearer");
    }

    #[test]
    fn message_content_is_always_an_array() {
        // 上游对字符串 content 回 4001 cannot unmarshal string，必须转数组
        let plan = build(&platform(), &backend(), &unified_with("你好"), &cred(), None).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        let content = messages[0]["content"].as_array().expect("content 必须是数组");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "你好");
    }

    #[test]
    fn system_prompt_is_prepended_as_array_content() {
        let mut unified = unified_with("hi");
        unified.system = Some("你是助手".into());
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"].is_array());
    }

    #[test]
    fn developer_role_is_rewritten_to_system() {
        // 统一格式没有 developer 角色（parse 阶段已归一），这里验证 System 保持 system
        let mut unified = unified_with("hi");
        unified.messages.insert(
            0,
            UnifiedMessage {
                role: Role::System,
                content: Some(serde_json::json!("dev")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_blocks: vec![],
                raw_blocks: vec![],
            },
        );
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let messages = plan.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
    }

    #[test]
    fn config_name_comes_from_backend_model_id() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        assert_eq!(plan.body["config_name"], "glm-5.2");
        assert_eq!(plan.body["function"], DEFAULT_FUNCTION);
    }

    #[test]
    fn tool_parameters_are_serialized_as_json_string() {
        // 上游要求 parameters 是字符串，传 object 会 400
        let mut unified = unified_with("hi");
        unified.tools = Some(vec![UnifiedTool {
            id: "t1".into(),
            tool_type: "function".into(),
            function: UnifiedFunction {
                name: "get_weather".into(),
                description: Some("查天气".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "city": { "type": "string" } }
                })),
            },
        }]);
        let plan = build(&platform(), &backend(), &unified, &cred(), None).unwrap();
        let params = &plan.body["tools"][0]["function"]["parameters"];
        assert!(params.is_string(), "parameters 必须是 JSON 字符串，实际：{params}");
        let parsed: Value = serde_json::from_str(params.as_str().unwrap()).unwrap();
        assert_eq!(parsed["type"], "object");
        assert_eq!(plan.body["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn tool_choice_object_is_normalized() {
        assert_eq!(
            normalize_tool_choice(&ToolChoice::None("auto".into())),
            json!("auto")
        );
        assert_eq!(
            normalize_tool_choice(&ToolChoice::Object {
                r#type: "required".into(),
                function: None
            }),
            json!("required")
        );
        assert_eq!(
            normalize_tool_choice(&ToolChoice::Object {
                r#type: "function".into(),
                function: Some(crate::protocol::types::ToolFunction { name: "f1".into() })
            }),
            json!("f1")
        );
        assert_eq!(
            normalize_tool_choice(&ToolChoice::Object {
                r#type: "weird".into(),
                function: None
            }),
            json!("auto")
        );
    }

    #[test]
    fn always_sse_and_force_stream_are_set() {
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        assert!(plan.force_stream, "Trae 必须显式 stream");
        assert!(plan.always_sse, "Trae 无论 stream 与否都回 SSE");
        assert_eq!(plan.body["stream"], true);
    }

    #[test]
    fn device_headers_are_only_sent_when_present() {
        // 如实携带：凭据里没有就不发，绝不生成假指纹
        let plan = build(&platform(), &backend(), &unified_with("hi"), &cred(), None).unwrap();
        assert!(!plan.headers.iter().any(|(k, _)| k == "x-device-id"));

        let with_device = AdapterCredential {
            device_id: Some("dev-1"),
            machine_id: Some("mach-1"),
            ..cred()
        };
        let plan = build(&platform(), &backend(), &unified_with("hi"), &with_device, None).unwrap();
        assert!(plan.headers.iter().any(|(k, v)| k == "x-device-id" && v == "dev-1"));
        assert!(plan.headers.iter().any(|(k, v)| k == "x-machine-id" && v == "mach-1"));
    }

    #[test]
    fn empty_base_url_is_rejected() {
        let mut p = platform();
        p.base_url = "  ".into();
        let err = build(&p, &backend(), &unified_with("hi"), &cred(), None);
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("base_url"));
    }

    #[test]
    fn param_overrides_win_over_defaults() {
        let mut b = backend();
        b.param_overrides = serde_json::json!({ "max_tokens": 999, "custom_flag": true });
        let plan = build(&platform(), &b, &unified_with("hi"), &cred(), Some(100)).unwrap();
        assert_eq!(plan.body["max_tokens"], 999);
        assert_eq!(plan.body["custom_flag"], true);
    }

    #[test]
    fn business_error_is_parsed_from_sse_payload() {
        let err = parse_business_error(r#"{"code":4008,"message":"quota exceeded"}"#).unwrap();
        assert_eq!(err.code, Some(4008));
        assert_eq!(err.message, "quota exceeded");

        let err = parse_business_error(r#"{"error":{"code":4017,"message":"risk"}}"#).unwrap();
        assert_eq!(err.code, Some(4017));

        // code=0 是成功，不是错误
        assert!(parse_business_error(r#"{"code":0}"#).is_none());
        // 非 JSON 不算业务错误
        assert!(parse_business_error("not json").is_none());
    }
}
