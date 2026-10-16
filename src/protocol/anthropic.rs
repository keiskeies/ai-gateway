use serde::{Deserialize, Serialize};
use super::content;
use super::types::*;

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
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
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
                    });
                }
            }
            "assistant" => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                let mut reasoning_text = String::new();
                let mut tool_calls = None;
                if let Some(arr) = msg.content.as_array() {
                    let mut tcs = Vec::new();
                    for block in arr {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") => blocks.push(block.clone()),
                            Some("thinking") => {
                                if let Some(t) = block.get("thinking").and_then(|v| v.as_str()) { reasoning_text.push_str(t); }
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
                messages.push(UnifiedMessage { role: Role::Assistant, content: msg_content, reasoning_content, tool_calls, tool_call_id: None, name: None });
            }
            _ => {}
        }
    }

    // 从 extra 中移除已由结构体字段处理的 key，避免重复
    let mut extra = req.extra;
    for key in &["model", "messages", "system", "max_tokens", "temperature", "top_p", "stream", "stop_sequences", "tools", "tool_choice"] {
        extra.remove(*key);
    }

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
        extra,
    })
}

/// 统一格式 → Anthropic 请求
pub fn to_request(unified: &UnifiedRequest, target_model: &str, default_max_tokens: Option<u32>, unknown: crate::models::route::UnknownParams) -> serde_json::Value {
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
                let content = msg.content.as_ref().unwrap_or(&empty);
                let mut blocks = content::openai_content_to_anthropic_blocks(content);
                if let Some(tcs) = &msg.tool_calls {
                    for tc in tcs {
                        let input: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
                        blocks.push(serde_json::json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": input}));
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
    req.insert("max_tokens".into(), serde_json::to_value(unified.max_tokens.or(default_max_tokens).unwrap_or(4096)).unwrap());
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
    // 合并 extra 中的未知字段（已知字段优先，不覆盖）；Drop 策略下丢弃——
    // Anthropic 严格校验未知字段，OpenAI 特有参数（response_format/seed/logprobs 等）必须剥离
    if unknown != crate::models::route::UnknownParams::Drop {
        for (k, v) in &unified.extra {
            if !req.contains_key(k) {
                req.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::Value::Object(req)
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

    for block in &resp.content {
        match block {
            AnthropicContentBlock::Text { text } => {
                match content.as_mut() {
                    Some(serde_json::Value::String(s)) => s.push_str(text),
                    _ => content = Some(serde_json::Value::String(text.clone())),
                }
            }
            AnthropicContentBlock::ToolUse { id, name, input } => {
                tc_list.push(UnifiedToolCall {
                    id: id.clone(), call_type: "function".to_string(),
                    function: UnifiedFunctionCall { name: name.clone(), arguments: serde_json::to_string(input).unwrap_or_default() },
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
            message: Some(UnifiedMessage { role: Role::Assistant, content, reasoning_content: None, tool_calls, tool_call_id: None, name: None }),
            finish_reason,
        }],
        usage: Some(UnifiedUsage { prompt_tokens: resp.usage.input_tokens, completion_tokens: resp.usage.output_tokens, total_tokens: resp.usage.input_tokens + resp.usage.output_tokens }),
    })
}

/// 统一格式 → Anthropic 响应
pub fn to_response(unified: &UnifiedResponse) -> serde_json::Value {
    let choice = unified.choices.first();
    let msg = choice.and_then(|c| c.message.as_ref());

    let mut content_blocks: Vec<serde_json::Value> = Vec::new();
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
