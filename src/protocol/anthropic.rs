use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use super::content;
use super::reasoning::{self, MaxTokensField, ReasoningTarget};
use super::types::*;
use crate::models::proxy::Protocol;
use crate::models::route::{ReasoningStyle, UnknownParams};
use crate::proxy::reasoning_cache;

// ========== Anthropic 格式定义 ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    /// system 可以是字符串，也可以是内容块数组（Claude Code 发送的是数组形态）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<serde_json::Value>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// 保留未知字段，确保端到端透传
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub resp_type: String,
    pub role: String,
    /// 按原始 JSON 保存，避免新增块类型（thinking / redacted_thinking）导致反序列化失败
    pub content: Vec<Value>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// 命中提示词缓存的 token 数（注意：不计入 input_tokens，unified 口径需加回）
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
    /// 缓存写入 token 数（一次性成本，同样不计入 input_tokens）
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
}

// ========== 转换实现 ==========

/// Anthropic 请求 → 统一格式
pub fn parse_request(raw: serde_json::Value) -> Result<UnifiedRequest, String> {
    let req: AnthropicRequest = serde_json::from_value(raw)
        .map_err(|e| format!("Invalid Anthropic request: {}", e))?;

    let mut messages = Vec::new();

    for msg in req.messages {
        match msg.role.as_str() {
            "user" => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                if let Some(arr) = msg.content.as_array() {
                    for block in arr {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("tool_result") => {
                                let tool_use_id = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                // tool_result 的 content 可以是字符串或内容块数组（含 text/image）
                                let text = block.get("content").map(content::extract_text).unwrap_or_default();
                                messages.push(UnifiedMessage {
                                    role: Role::Tool,
                                    content: Some(serde_json::Value::String(text)),
                                    reasoning_content: None,
                                    tool_calls: None,
                                    tool_call_id: Some(tool_use_id),
                                    name: None,
                                    reasoning_blocks: Vec::new(),
                                    raw_blocks: arr.clone(),
                                });
                            }
                            Some("text") | Some("image") => blocks.push(block.clone()),
                            _ => {}
                        }
                    }
                } else if let Some(text) = msg.content.as_str() {
                    blocks.push(serde_json::json!({"type": "text", "text": text}));
                }
                if !blocks.is_empty() {
                    messages.push(UnifiedMessage {
                        role: Role::User,
                        content: Some(content::anthropic_blocks_to_openai(&blocks)),
                        reasoning_content: None,
                        tool_calls: None, tool_call_id: None, name: None,
                        reasoning_blocks: Vec::new(),
                        raw_blocks: Vec::new(),
                    });
                }
            }
            "assistant" => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                let mut reasoning_text = String::new();
                let mut reasoning_blocks: Vec<Value> = Vec::new();
                let mut raw_blocks: Vec<Value> = Vec::new();
                let mut tool_calls = None;
                if let Some(arr) = msg.content.as_array() {
                    raw_blocks = arr.clone();
                    let mut tcs = Vec::new();
                    for block in arr {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") => blocks.push(block.clone()),
                            Some("thinking") => {
                                if let Some(t) = block.get("thinking").and_then(|v| v.as_str()) {
                                    reasoning_text.push_str(t);
                                }
                                reasoning_blocks.push(block.clone());
                            }
                            Some("redacted_thinking") => {
                                reasoning_blocks.push(block.clone());
                            }
                            Some("tool_use") => {
                                tcs.push(UnifiedToolCall {
                                    id: block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    call_type: "function".to_string(),
                                    function: UnifiedFunctionCall {
                                        name: block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        arguments: serde_json::to_string(&block.get("input").cloned().unwrap_or(serde_json::Value::Null)).unwrap_or_default(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                    if !tcs.is_empty() { tool_calls = Some(tcs); }
                } else if let Some(text) = msg.content.as_str() {
                    blocks.push(serde_json::json!({"type": "text", "text": text}));
                }
                let msg_content = if blocks.is_empty() { None } else { Some(content::anthropic_blocks_to_openai(&blocks)) };
                let reasoning_content = if reasoning_text.is_empty() { None } else { Some(reasoning_text) };
                messages.push(UnifiedMessage {
                    role: Role::Assistant,
                    content: msg_content,
                    reasoning_content,
                    tool_calls,
                    tool_call_id: None,
                    name: None,
                    reasoning_blocks,
                    raw_blocks,
                });
            }
            _ => {}
        }
    }

    // 从 extra 中移除已由结构体字段处理的 key，避免重复
    let mut extra = req.extra;
    for key in &["model", "messages", "system", "max_tokens", "temperature", "top_p", "stream", "stop_sequences", "tools", "tool_choice"] {
        extra.remove(*key);
    }
    let reasoning = reasoning::take_anthropic_thinking(&mut extra);

    Ok(UnifiedRequest {
        // 数组形态的 system 统一拼接为纯文本（与 tool_result 的处理一致）
        model: req.model, messages,
        system: req.system.as_ref().map(content::extract_text).filter(|s| !s.is_empty()),
        max_tokens: Some(req.max_tokens), temperature: req.temperature, top_p: req.top_p,
        stream: req.stream, stop: req.stop_sequences,
        tools: req.tools.map(|tools| tools.into_iter().map(|t| UnifiedTool {
            id: format!("tool_{}", t.name), tool_type: "function".to_string(),
            function: UnifiedFunction { name: t.name, description: t.description, parameters: Some(t.input_schema) },
        }).collect()),
        tool_choice: req.tool_choice.and_then(|v| serde_json::from_value(v).ok()),
        source_protocol: Protocol::Anthropic,
        reasoning,
        max_tokens_field: MaxTokensField::MaxTokens,
        extra,
    })
}

/// 统一格式 → Anthropic 请求
pub fn to_request(
    unified: &UnifiedRequest,
    target_model: &str,
    default_max_tokens: Option<u32>,
    unknown: UnknownParams,
    target: &ReasoningTarget,
) -> serde_json::Value {
    let cfg = if target.style == ReasoningStyle::Disabled {
        None
    } else {
        unified.reasoning.as_ref()
    };
    let mut thinking_enabled = cfg.map(|c| c.is_enabled()).unwrap_or(false);
    let max_tokens = unified.max_tokens.or(default_max_tokens).unwrap_or(4096);
    let mut thinking_budget = cfg
        .and_then(|c| c.effective_budget_tokens())
        .unwrap_or(8192);

    if thinking_enabled {
        let clamped = thinking_budget
            .max(1024)
            .min(max_tokens.saturating_sub(1024));
        if clamped < 1024 {
            tracing::warn!(
                "max_tokens={} 无法容纳 thinking（至少需要 2048），本次关闭思考",
                max_tokens
            );
            thinking_enabled = false;
        } else {
            thinking_budget = clamped;
        }
    }

    // 工具轮次：历史 assistant 消息必须能带上真实 thinking 块，否则上游会 400。
    // 客户端已带真实块 → 直接用；否则查签名缓存；都拿不到 → fail-open 关闭思考。
    if thinking_enabled {
        for msg in &unified.messages {
            if msg.role != Role::Assistant {
                continue;
            }
            let has_tool_calls = msg.tool_calls.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
            if has_tool_calls && history_thinking_blocks(msg, target).is_none() {
                tracing::warn!(
                    "工具轮次缺少 thinking 块（backend={}, model={}），本次按 fail-open 关闭思考",
                    target.backend_id,
                    target.model_id
                );
                thinking_enabled = false;
                break;
            }
        }
    }

    let mut anthropic_messages = Vec::new();

    let empty = serde_json::Value::Null;
    for msg in &unified.messages {
        match msg.role {
            Role::User => {
                let content = msg.content.as_ref().unwrap_or(&empty);
                let mut blocks = content::openai_content_to_anthropic_blocks(content);
                if blocks.is_empty() { blocks.push(serde_json::json!({"type": "text", "text": ""})); }
                anthropic_messages.push(AnthropicMessage { role: "user".to_string(), content: serde_json::Value::Array(blocks) });
            }
            Role::Assistant => {
                let mut blocks: Vec<Value> = if !msg.raw_blocks.is_empty() {
                    // 同协议优先原样回放；合成签名块必须剥离，绝不能发给真实 Anthropic
                    msg.raw_blocks.iter()
                        .filter(|block| !is_synthetic_thinking_block(block))
                        .cloned()
                        .collect()
                } else {
                    let content = msg.content.as_ref().unwrap_or(&empty);
                    let mut blocks = content::openai_content_to_anthropic_blocks(content);
                    if let Some(tcs) = &msg.tool_calls {
                        for tc in tcs {
                            let input: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
                            blocks.push(serde_json::json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": input}));
                        }
                    }
                    blocks
                };
                if !contains_thinking_block(&blocks) {
                    if let Some(mut extra_blocks) = history_thinking_blocks(msg, target) {
                        extra_blocks.append(&mut blocks);
                        blocks = extra_blocks;
                    }
                }
                if blocks.is_empty() { blocks.push(serde_json::json!({"type": "text", "text": ""})); }
                anthropic_messages.push(AnthropicMessage { role: "assistant".to_string(), content: serde_json::Value::Array(blocks) });
            }
            Role::Tool => {
                // Anthropic: tool_result 作为 user message 的 content block
                let content = msg.content.as_ref().unwrap_or(&empty);
                let mut blocks = content::openai_content_to_anthropic_blocks(content);
                let result_content = match blocks.len() {
                    0 => serde_json::Value::String(String::new()),
                    1 if blocks[0].get("type").and_then(|t| t.as_str()) == Some("text") => {
                        blocks.remove(0).get("text").cloned().unwrap_or(serde_json::Value::String(String::new()))
                    }
                    _ => serde_json::Value::Array(blocks),
                };
                let tool_result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "content": result_content
                });
                anthropic_messages.push(AnthropicMessage { role: "user".to_string(), content: serde_json::Value::Array(vec![tool_result]) });
            }
            Role::System => { /* system 通过顶层字段传递 */ }
        }
    }

    let mut req = serde_json::Map::new();
    req.insert("model".into(), serde_json::Value::String(target_model.to_string()));
    req.insert("messages".into(), serde_json::to_value(&anthropic_messages).unwrap());
    // Anthropic 必填 max_tokens：客户端未传时依次用路由默认值、4096 兜底
    req.insert("max_tokens".into(), serde_json::to_value(max_tokens).unwrap());
    if let Some(v) = &unified.system { req.insert("system".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = unified.temperature { req.insert("temperature".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = unified.top_p { req.insert("top_p".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = unified.stream { req.insert("stream".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = &unified.stop { req.insert("stop_sequences".into(), serde_json::to_value(v).unwrap()); }
    // OpenAI 的 tool_choice=none：Anthropic 无对应值，直接不携带工具
    let tool_choice_none = matches!(
        &unified.tool_choice,
        Some(crate::protocol::types::ToolChoice::None(s)) if s == "none"
    ) || matches!(
        &unified.tool_choice,
        Some(crate::protocol::types::ToolChoice::Object { r#type, .. }) if r#type == "none"
    );
    if !tool_choice_none {
        if let Some(tools) = &unified.tools {
            req.insert("tools".into(), serde_json::to_value(tools.iter().map(|t| AnthropicTool {
                name: t.function.name.clone(), description: t.function.description.clone(),
                input_schema: t.function.parameters.clone().unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
            }).collect::<Vec<_>>()).unwrap());
        }
        // OpenAI tool_choice 格式 → Anthropic 格式（"auto"/{"type":"function"} 原样透传会被 Anthropic 400 拒绝）
        if let Some(tc) = unified.tool_choice.as_ref().and_then(convert_tool_choice) {
            req.insert("tool_choice".into(), tc);
        }
    }

    // thinking 必须在 max_tokens 确定后再写入，并满足 budget < max_tokens
    if thinking_enabled {
        let mut obj = cfg
            .and_then(|c| c.raw.get("thinking"))
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        obj.insert("type".into(), serde_json::json!("enabled"));
        obj.insert("budget_tokens".into(), serde_json::json!(thinking_budget));
        req.insert("thinking".into(), serde_json::Value::Object(obj));
        // Anthropic 在 thinking 开启时拒绝采样参数
        req.remove("temperature");
        req.remove("top_p");
        req.remove("top_k");
    } else if cfg.map(|c| c.enabled == Some(false)).unwrap_or(false) {
        req.insert("thinking".into(), serde_json::json!({"type": "disabled"}));
    }

    for (k, v) in &unified.extra {
        if req.contains_key(k) {
            continue;
        }
        if thinking_enabled && k == "top_k" {
            continue;
        }
        if reasoning::should_forward_extra(&unknown, unified.source_protocol, Protocol::Anthropic, k) {
            req.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(req)
}

fn is_synthetic_thinking_block(block: &Value) -> bool {
    block.get("type").and_then(|t| t.as_str()) == Some("thinking")
        && block
            .get("signature")
            .and_then(|s| s.as_str())
            .map(reasoning::is_synthetic_signature)
            .unwrap_or(false)
}

fn contains_thinking_block(blocks: &[Value]) -> bool {
    blocks.iter().any(|block| {
        matches!(
            block.get("type").and_then(|t| t.as_str()),
            Some("thinking") | Some("redacted_thinking")
        ) && !is_synthetic_thinking_block(block)
    })
}

/// 历史 assistant 消息可用的真实 thinking 块：客户端原始块优先，其次签名缓存。
fn history_thinking_blocks(msg: &UnifiedMessage, target: &ReasoningTarget) -> Option<Vec<Value>> {
    let from_client: Vec<Value> = msg
        .reasoning_blocks
        .iter()
        .chain(msg.raw_blocks.iter())
        .filter(|block| {
            matches!(
                block.get("type").and_then(|t| t.as_str()),
                Some("thinking") | Some("redacted_thinking")
            ) && !is_synthetic_thinking_block(block)
        })
        .cloned()
        .collect();
    if !from_client.is_empty() {
        return Some(from_client);
    }
    let tool_id = msg
        .tool_calls
        .as_ref()
        .and_then(|tcs| tcs.first())
        .map(|tc| tc.id.clone())?;
    reasoning_cache::lookup(&tool_id, &target.backend_id, &target.model_id)
}

/// OpenAI tool_choice → Anthropic tool_choice：
/// - "auto" → {"type":"auto"}
/// - "required" → {"type":"any"}
/// - "none" → None（调用方应同时不携带 tools）
/// - {"type":"function","function":{name}} → {"type":"tool","name":...}
fn convert_tool_choice(tc: &crate::protocol::types::ToolChoice) -> Option<serde_json::Value> {
    use crate::protocol::types::ToolChoice;
    match tc {
        ToolChoice::None(s) => match s.as_str() {
            "required" => Some(serde_json::json!({"type": "any"})),
            "none" => None,
            _ => Some(serde_json::json!({"type": "auto"})),
        },
        ToolChoice::Object { r#type, function } => match r#type.as_str() {
            "function" => function.as_ref().map(|f| serde_json::json!({"type": "tool", "name": f.name})),
            "required" => Some(serde_json::json!({"type": "any"})),
            "none" => None,
            _ => Some(serde_json::json!({"type": "auto"})),
        },
    }
}

/// Anthropic 响应 → 统一格式
pub fn parse_response(raw: serde_json::Value) -> Result<UnifiedResponse, String> {
    let resp: AnthropicResponse = serde_json::from_value(raw)
        .map_err(|e| format!("Invalid Anthropic response: {}", e))?;

    let mut content: Option<serde_json::Value> = None;
    let mut tool_calls = None;
    let mut tc_list = Vec::new();
    let mut reasoning_text = String::new();
    let mut reasoning_blocks = Vec::new();
    let raw_blocks = resp.content.clone();

    for block in &resp.content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                match content.as_mut() {
                    Some(serde_json::Value::String(s)) => s.push_str(text),
                    _ => content = Some(serde_json::Value::String(text.to_string())),
                }
            }
            Some("thinking") => {
                if let Some(text) = block.get("thinking").and_then(|t| t.as_str()) {
                    reasoning_text.push_str(text);
                }
                reasoning_blocks.push(block.clone());
            }
            Some("redacted_thinking") => {
                reasoning_blocks.push(block.clone());
            }
            Some("tool_use") => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                tc_list.push(UnifiedToolCall {
                    id, call_type: "function".to_string(),
                    function: UnifiedFunctionCall { name, arguments: serde_json::to_string(&input).unwrap_or_default() },
                });
            }
            _ => {}
        }
    }
    if !tc_list.is_empty() { tool_calls = Some(tc_list); }

    let finish_reason = resp.stop_reason.map(|r| match r.as_str() {
        "end_turn" => "stop".to_string(),
        "max_tokens" => "length".to_string(),
        "tool_use" => "tool_calls".to_string(),
        _ => r,
    });

    Ok(UnifiedResponse {
        id: resp.id, model: resp.model,
        choices: vec![UnifiedChoice {
            index: 0,
            message: Some(UnifiedMessage {
                role: Role::Assistant,
                content,
                reasoning_content: if reasoning_text.is_empty() { None } else { Some(reasoning_text) },
                tool_calls,
                tool_call_id: None,
                name: None,
                reasoning_blocks,
                raw_blocks,
            }),
            finish_reason,
        }],
        // unified 口径对齐 OpenAI：prompt_tokens 为总提示词（Anthropic 的 input_tokens 不含缓存部分，需加回）
        usage: Some(UnifiedUsage {
            prompt_tokens: resp.usage.input_tokens + resp.usage.cache_read_input_tokens.unwrap_or(0) + resp.usage.cache_creation_input_tokens.unwrap_or(0),
            completion_tokens: resp.usage.output_tokens,
            total_tokens: resp.usage.input_tokens + resp.usage.output_tokens + resp.usage.cache_read_input_tokens.unwrap_or(0) + resp.usage.cache_creation_input_tokens.unwrap_or(0),
            cache_read_tokens: resp.usage.cache_read_input_tokens,
            cache_write_tokens: resp.usage.cache_creation_input_tokens,
        }),
    })
}

/// 统一格式 → Anthropic 响应
pub fn to_response(unified: &UnifiedResponse) -> serde_json::Value {
    let choice = unified.choices.first();
    let msg = choice.and_then(|c| c.message.as_ref());

    let mut content_blocks: Vec<serde_json::Value> = Vec::new();
    if let Some(msg) = msg {
        if !msg.reasoning_blocks.is_empty() {
            content_blocks.extend(msg.reasoning_blocks.iter().cloned());
        } else if let Some(reasoning) = msg.reasoning_content.as_ref().filter(|s| !s.is_empty()) {
            // OpenAI 上游没有真实 Anthropic 签名，生成内部合成签名；请求侧会识别并剥离
            content_blocks.push(json!({
                "type": "thinking",
                "thinking": reasoning,
                "signature": reasoning::synthetic_signature()
            }));
        }
    }
    if let Some(content) = msg.and_then(|m| m.content.as_ref()) {
        content_blocks.extend(content::openai_content_to_anthropic_blocks(content));
    }
    if let Some(tcs) = msg.and_then(|m| m.tool_calls.as_ref()) {
        for tc in tcs {
            let input: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
            content_blocks.push(serde_json::json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": input}));
        }
    }
    if content_blocks.is_empty() { content_blocks.push(serde_json::json!({"type": "text", "text": ""})); }

    let stop_reason = choice.and_then(|c| c.finish_reason.as_ref()).map(|r| match r.as_str() {
        "stop" => "end_turn", "length" => "max_tokens", "tool_calls" => "tool_use", _ => r.as_str(),
    });

    let usage = unified.usage.as_ref();
    serde_json::json!({
        "id": unified.id,
        "type": "message",
        "role": "assistant",
        "content": content_blocks,
        "model": unified.model,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": usage.map(|u| u.prompt_tokens).unwrap_or(0),
            "output_tokens": usage.map(|u| u.completion_tokens).unwrap_or(0),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::route::{ReasoningEcho, ReasoningStyle};

    fn target(style: ReasoningStyle) -> ReasoningTarget {
        ReasoningTarget::new(style, "b1", "m1", ReasoningEcho::Drop)
    }

    #[test]
    fn thinking_request_is_forwarded_and_clamped() {
        let raw = json!({
            "model": "claude",
            "max_tokens": 8192,
            "temperature": 0.7,
            "thinking": {"type": "enabled", "budget_tokens": 32000},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let unified = parse_request(raw).unwrap();
        let body = to_request(&unified, "claude-upstream", None, UnknownParams::Auto, &target(ReasoningStyle::AnthropicThinking));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 7168);
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn openai_effort_maps_to_anthropic_thinking() {
        let raw = json!({
            "model": "virt",
            "max_tokens": 32768,
            "reasoning_effort": "high",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let unified = crate::protocol::openai::parse_request(raw).unwrap();
        let body = to_request(&unified, "claude-upstream", None, UnknownParams::Auto, &target(ReasoningStyle::AnthropicThinking));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16384);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn small_max_tokens_disables_thinking() {
        let raw = json!({
            "model": "virt",
            "max_tokens": 1024,
            "reasoning_effort": "high",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let unified = crate::protocol::openai::parse_request(raw).unwrap();
        let body = to_request(&unified, "claude-upstream", None, UnknownParams::Auto, &target(ReasoningStyle::AnthropicThinking));
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn raw_thinking_blocks_are_replayed() {
        let raw = json!({
            "model": "claude",
            "max_tokens": 8192,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "pondering", "signature": "real-sig"},
                    {"type": "text", "text": "calling"},
                    {"type": "tool_use", "id": "toolu_1", "name": "f", "input": {}}
                ]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]}
            ]
        });
        let unified = parse_request(raw).unwrap();
        let body = to_request(&unified, "claude-upstream", None, UnknownParams::Auto, &target(ReasoningStyle::AnthropicThinking));
        assert_eq!(body["messages"][1]["content"][0]["signature"], "real-sig");
        assert_eq!(body["messages"][1]["content"][1]["text"], "calling");
    }

    #[test]
    fn synthetic_thinking_is_stripped_for_real_anthropic() {
        let raw = json!({
            "model": "claude",
            "max_tokens": 8192,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "x", "signature": "gw_sig_abc"},
                    {"type": "text", "text": "calling"},
                    {"type": "tool_use", "id": "toolu_missing", "name": "f", "input": {}}
                ]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_missing", "content": "ok"}]}
            ]
        });
        let unified = parse_request(raw).unwrap();
        let body = to_request(&unified, "claude-upstream", None, UnknownParams::Auto, &target(ReasoningStyle::AnthropicThinking));
        assert!(body.get("thinking").is_none());
        assert_eq!(body["messages"][1]["content"][0]["type"], "text");
    }

    #[test]
    fn response_thinking_blocks_are_parsed() {
        let raw = json!({
            "id": "msg_1", "type": "message", "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "think", "signature": "sig"},
                {"type": "redacted_thinking", "data": "opaque"},
                {"type": "text", "text": "answer"},
                {"type": "tool_use", "id": "toolu_1", "name": "f", "input": {"a": 1}}
            ],
            "model": "claude",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 5, "output_tokens": 7}
        });
        let unified = parse_response(raw).unwrap();
        let msg = unified.choices[0].message.as_ref().unwrap();
        assert_eq!(msg.reasoning_content.as_deref(), Some("think"));
        assert_eq!(msg.reasoning_blocks.len(), 2);
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }
}
