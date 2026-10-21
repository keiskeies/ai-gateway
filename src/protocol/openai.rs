use serde::{Deserialize, Serialize};
use super::content;
use super::reasoning::{self, MaxTokensField, ReasoningTarget};
use super::types::*;
use crate::models::proxy::Protocol;
use crate::models::route::{ReasoningEcho, ReasoningStyle, UnknownParams};

// ========== OpenAI 格式定义 ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// 保留未知字段，确保端到端透传
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<serde_json::Value>,
    /// OpenRouter 等上游把思考内容放在 reasoning 字段
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<serde_json::Value>,
    /// OpenRouter 结构化思考明细
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAIFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAIFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChatResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub message: OpenAIMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// 部分 OpenAI 兼容上游（如 Ollama、部分第三方网关）的 usage 不带 total_tokens，缺失时按 0 处理
    #[serde(default)]
    pub total_tokens: u32,
    /// 提示词缓存命中明细（OpenAI 标准字段；DeepSeek 老格式用 prompt_cache_hit_tokens，见下方兜底）
    #[serde(default)]
    pub prompt_tokens_details: Option<OpenAIPromptTokensDetails>,
    /// DeepSeek 老格式：命中缓存的 prompt token 数
    #[serde(default)]
    pub prompt_cache_hit_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIPromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
    /// 部分上游（OpenAI、OpenRouter 等）会在流式最后一个 chunk 中携带 usage
    #[serde(default)]
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamChoice {
    pub index: u32,
    pub delta: OpenAIMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIModelsResponse {
    pub object: String,
    pub data: Vec<OpenAIModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIModel {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

// ========== 转换实现 ==========

/// OpenAI 请求 → 统一格式
pub fn parse_request(raw: serde_json::Value) -> Result<UnifiedRequest, String> {
    let req: OpenAIChatRequest = serde_json::from_value(raw)
        .map_err(|e| format!("Invalid OpenAI request: {}", e))?;

    let mut messages = Vec::new();
    let mut system = None;

    for msg in req.messages {
        match msg.role.as_str() {
            "system" => {
                system = msg.content.as_ref().map(content::extract_text);
                // 也添加到 messages 中保留（content 原样保留，含多模态数组）
                messages.push(UnifiedMessage {
                    role: Role::System,
                    content: msg.content.clone(),
                    reasoning_content: None,
                    tool_calls: None, tool_call_id: None, name: None,
                    reasoning_blocks: Vec::new(), raw_blocks: Vec::new(),
                });
            }
            "user" => messages.push(UnifiedMessage {
                role: Role::User,
                content: msg.content.clone(),
                reasoning_content: None,
                tool_calls: None, tool_call_id: None, name: None,
                reasoning_blocks: Vec::new(), raw_blocks: Vec::new(),
            }),
            "assistant" => messages.push(UnifiedMessage {
                role: Role::Assistant,
                content: msg.content.clone(),
                reasoning_content: reasoning_text(&msg),
                tool_calls: msg.tool_calls.map(|tcs| tcs.into_iter().map(|tc| UnifiedToolCall {
                    id: tc.id, call_type: tc.call_type,
                    function: UnifiedFunctionCall { name: tc.function.name, arguments: tc.function.arguments },
                }).collect()),
                tool_call_id: None, name: None,
                reasoning_blocks: Vec::new(), raw_blocks: Vec::new(),
            }),
            "tool" => messages.push(UnifiedMessage {
                role: Role::Tool,
                content: msg.content.clone(),
                reasoning_content: None,
                tool_calls: None, tool_call_id: msg.tool_call_id, name: msg.name,
                reasoning_blocks: Vec::new(), raw_blocks: Vec::new(),
            }),
            _ => {}
        }
    }

    let stop = req.stop.and_then(|s| {
        if let Some(arr) = s.as_array() { Some(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()) } else { s.as_str().map(|s| vec![s.to_string()]) }
    });

    // 从 extra 中移除已由结构体字段处理的 key，避免重复
    let mut extra = req.extra;
    for key in &["model", "messages", "max_tokens", "temperature", "top_p", "stream", "stop", "tools", "tool_choice"] {
        extra.remove(*key);
    }

    // max_completion_tokens 是 max_tokens 的别名；记录原字段名，OpenAI 目标原样输出
    let mut max_tokens = req.max_tokens;
    let mut max_tokens_field = MaxTokensField::MaxTokens;
    if let Some(value) = extra.remove("max_completion_tokens") {
        if let Some(n) = value.as_u64() {
            max_tokens = Some(n.min(u32::MAX as u64) as u32);
            max_tokens_field = MaxTokensField::MaxCompletionTokens;
        }
    }

    let reasoning = reasoning::take_openai_reasoning(&mut extra);

    Ok(UnifiedRequest {
        model: req.model, messages, system,
        max_tokens, temperature: req.temperature, top_p: req.top_p,
        stream: req.stream, stop,
        tools: req.tools.map(|tools| tools.into_iter().map(|t| UnifiedTool {
            id: format!("tool_{}", t.function.name), tool_type: t.tool_type,
            function: UnifiedFunction { name: t.function.name, description: t.function.description, parameters: t.function.parameters },
        }).collect()),
        tool_choice: req.tool_choice.and_then(|v| serde_json::from_value(v).ok()),
        source_protocol: Protocol::OpenAI,
        reasoning,
        max_tokens_field,
        extra,
    })
}

fn reasoning_text(msg: &OpenAIMessage) -> Option<String> {
    if let Some(text) = msg.reasoning_content.as_ref().and_then(value_text) {
        return Some(text);
    }
    if let Some(text) = msg.reasoning.as_ref().and_then(value_text) {
        return Some(text);
    }
    msg.reasoning_details.as_ref().and_then(value_text)
}

fn value_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    out.push_str(text);
                } else if let Some(text) = item.get("summary").and_then(|v| v.as_str()) {
                    out.push_str(text);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

/// 统一格式 → OpenAI 请求
pub fn to_request(
    unified: &UnifiedRequest,
    target_model: &str,
    unknown: UnknownParams,
    target: &ReasoningTarget,
) -> serde_json::Value {
    let keep_reasoning = matches!(target.echo, ReasoningEcho::Keep);
    let messages: Vec<OpenAIMessage> = unified.messages.iter().map(|m| {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        OpenAIMessage {
            role: role.to_string(),
            content: forward_content(m),
            reasoning_content: m.reasoning_content.as_ref()
                .filter(|_| keep_reasoning)
                .map(|s| serde_json::Value::String(s.clone())),
            reasoning: None,
            reasoning_details: None,
            tool_calls: m.tool_calls.as_ref().map(|tcs| tcs.iter().map(|tc| OpenAIToolCall {
                id: tc.id.clone(), call_type: tc.call_type.clone(),
                function: OpenAIFunctionCall { name: tc.function.name.clone(), arguments: tc.function.arguments.clone() },
            }).collect()),
            tool_call_id: m.tool_call_id.clone(),
            name: m.name.clone(),
        }
    }).collect();

    let mut req = serde_json::Map::new();
    req.insert("model".into(), serde_json::Value::String(target_model.to_string()));
    req.insert("messages".into(), serde_json::to_value(&messages).unwrap());
    if let Some(v) = unified.max_tokens {
        let field = match unified.max_tokens_field {
            MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
            MaxTokensField::MaxTokens => "max_tokens",
        };
        req.insert(field.into(), serde_json::to_value(v).unwrap());
    }
    if let Some(v) = unified.temperature { req.insert("temperature".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = unified.top_p { req.insert("top_p".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = unified.stream { req.insert("stream".into(), serde_json::to_value(v).unwrap()); }
    if let Some(v) = &unified.stop { req.insert("stop".into(), serde_json::to_value(v).unwrap()); }
    if let Some(tools) = &unified.tools {
        req.insert("tools".into(), serde_json::to_value(tools.iter().map(|t| OpenAITool {
            tool_type: t.tool_type.clone(),
            function: OpenAIFunction { name: t.function.name.clone(), description: t.function.description.clone(), parameters: t.function.parameters.clone() },
        }).collect::<Vec<_>>()).unwrap());
    }
    if let Some(tc) = &unified.tool_choice {
        req.insert("tool_choice".into(), serde_json::to_value(tc).unwrap());
    }

    apply_reasoning(&mut req, unified, target);

    // 合并 extra 中的未知字段（已知字段优先，不覆盖）；按源/目标协议决定透传范围
    for (k, v) in &unified.extra {
        if !req.contains_key(k) && reasoning::should_forward_extra(
            &unknown, unified.source_protocol, Protocol::OpenAI, k,
        ) {
            req.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(req)
}

/// 按 backend 的思考风格写入出站参数；客户端未传思考配置时不主动开启。
fn apply_reasoning(
    req: &mut serde_json::Map<String, serde_json::Value>,
    unified: &UnifiedRequest,
    target: &ReasoningTarget,
) {
    if target.style == ReasoningStyle::Disabled {
        return;
    }
    let Some(cfg) = unified.reasoning.as_ref() else { return };
    let enabled = cfg.is_enabled();
    let budget = cfg.effective_budget_tokens();
    let effort = cfg.effective_effort();

    match target.style {
        ReasoningStyle::Auto | ReasoningStyle::Disabled => {}
        ReasoningStyle::OpenAiEffort => {
            if !enabled {
                if cfg.enabled == Some(false) {
                    req.insert("reasoning_effort".into(), serde_json::json!("none"));
                }
                return;
            }
            if let Some(raw) = cfg.raw.get("reasoning_effort") {
                req.insert("reasoning_effort".into(), raw.clone());
            }
            let value = effort.unwrap_or(reasoning::ReasoningEffort::Medium);
            req.insert("reasoning_effort".into(), serde_json::json!(value.as_str()));
        }
        ReasoningStyle::OpenRouter => {
            let mut obj = cfg.raw.get("reasoning")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            if !enabled {
                obj.insert("enabled".into(), serde_json::json!(false));
            } else {
                if let Some(effort) = &effort {
                    obj.insert("effort".into(), serde_json::json!(effort.as_str()));
                }
                if let Some(budget) = budget {
                    obj.insert("max_tokens".into(), serde_json::json!(budget));
                }
                if cfg.include_thoughts == Some(false) {
                    obj.insert("exclude".into(), serde_json::json!(true));
                }
            }
            req.insert("reasoning".into(), serde_json::Value::Object(obj));
        }
        ReasoningStyle::Qwen => {
            if enabled {
                req.insert("enable_thinking".into(), serde_json::json!(true));
                if let Some(budget) = budget {
                    req.insert("thinking_budget".into(), serde_json::json!(budget));
                }
            } else if cfg.enabled == Some(false) {
                req.insert("enable_thinking".into(), serde_json::json!(false));
            }
        }
        ReasoningStyle::Glm => {
            // GLM 只认 thinking.type，不认 Anthropic 的 budget_tokens
            if enabled {
                req.insert("thinking".into(), serde_json::json!({"type": "enabled"}));
            } else if cfg.enabled == Some(false) {
                req.insert("thinking".into(), serde_json::json!({"type": "disabled"}));
            }
        }
        ReasoningStyle::AnthropicThinking => {
            if enabled {
                let mut obj = serde_json::Map::new();
                obj.insert("type".into(), serde_json::json!("enabled"));
                if let Some(budget) = budget {
                    obj.insert("budget_tokens".into(), serde_json::json!(budget));
                }
                req.insert("thinking".into(), serde_json::Value::Object(obj));
            } else if cfg.enabled == Some(false) {
                req.insert("thinking".into(), serde_json::json!({"type": "disabled"}));
            }
        }
        ReasoningStyle::VllmQwen => {
            if enabled || cfg.enabled == Some(false) {
                let mut kwargs = req.get("chat_template_kwargs")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                kwargs.insert("enable_thinking".into(), serde_json::json!(enabled));
                req.insert("chat_template_kwargs".into(), serde_json::Value::Object(kwargs));
            }
        }
    }
}

/// 转发给上游时的 content 取值。
///
/// 原样透传（字符串或 OpenAI 多模态数组）；当 content 缺失/为 null 时补一个最小占位，
/// 因为部分上游（如 agnes）要求 user/system/tool 消息必须带 `content` 字段且不能是空串：
/// 缺字段报 `missing field content`，空串报 `message content cannot be empty`，单个空格则放行。
/// 唯一例外：assistant 消息带 tool_calls 时保持省略 content（OpenAI 标准写法）。
fn forward_content(m: &UnifiedMessage) -> Option<serde_json::Value> {
    match &m.content {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => {
            let has_tool_calls = m.tool_calls.as_ref().map(|tcs| !tcs.is_empty()).unwrap_or(false);
            if m.role == Role::Assistant && has_tool_calls {
                None
            } else {
                Some(serde_json::Value::String(" ".to_string()))
            }
        }
    }
}

/// OpenAI 响应 → 统一格式
pub fn parse_response(raw: serde_json::Value) -> Result<UnifiedResponse, String> {
    let resp: OpenAIChatResponse = serde_json::from_value(raw)
        .map_err(|e| format!("Invalid OpenAI response: {}", e))?;

    Ok(UnifiedResponse {
        id: resp.id, model: resp.model,
        choices: resp.choices.into_iter().map(|c| {
            let reasoning = reasoning_text(&c.message);
            UnifiedChoice {
                index: c.index,
                message: Some(UnifiedMessage {
                    role: match c.message.role.as_str() {
                        "assistant" => Role::Assistant, "user" => Role::User,
                        "system" => Role::System, _ => Role::Assistant,
                    },
                    content: extract_message_content(&c.message.content, &c.message.reasoning_content),
                    reasoning_content: reasoning,
                    tool_calls: c.message.tool_calls.map(|tcs| tcs.into_iter().map(|tc| UnifiedToolCall {
                        id: tc.id, call_type: tc.call_type,
                        function: UnifiedFunctionCall { name: tc.function.name, arguments: tc.function.arguments },
                    }).collect()),
                    tool_call_id: None, name: None,
                    reasoning_blocks: Vec::new(), raw_blocks: Vec::new(),
                }),
                finish_reason: c.finish_reason,
            }
        }).collect(),
        usage: resp.usage.map(|u| UnifiedUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            // 缓存命中：优先 OpenAI 标准 prompt_tokens_details.cached_tokens，兜底 DeepSeek 老格式
            cache_read_tokens: u.prompt_tokens_details.and_then(|d| d.cached_tokens).or(u.prompt_cache_hit_tokens),
            cache_write_tokens: None,
        }),
    })
}

/// Extract content from OpenAI message content field.
/// Keeps strings and content arrays as-is, normalizes null to an empty string,
/// and falls back to reasoning_content if content is absent (e.g. NVIDIA reasoning models).
fn extract_message_content(content: &Option<serde_json::Value>, reasoning_content: &Option<serde_json::Value>) -> Option<serde_json::Value> {
    fn normalize(value: &Option<serde_json::Value>) -> Option<serde_json::Value> {
        match value {
            None => None,
            Some(serde_json::Value::Null) => Some(serde_json::Value::String(String::new())),
            Some(v) => Some(v.clone()),
        }
    }
    normalize(content).or_else(|| normalize(reasoning_content))
}

/// 统一格式 → OpenAI 响应
pub fn to_response(unified: &UnifiedResponse) -> serde_json::Value {
    let choices: Vec<serde_json::Value> = unified.choices.iter().map(|c| {
        let msg = c.message.as_ref();
        let role = msg.map(|m| match m.role { Role::Assistant => "assistant", Role::User => "user", Role::System => "system", Role::Tool => "tool" }).unwrap_or("assistant");
        let content_val = match msg.and_then(|m| m.content.as_ref()) {
            Some(v) => v.clone(),
            None => serde_json::Value::Null,
        };
        let mut message_obj = serde_json::Map::new();
        message_obj.insert("role".into(), serde_json::Value::String(role.to_string()));
        message_obj.insert("content".into(), content_val);
        if let Some(rc) = msg.and_then(|m| m.reasoning_content.as_ref()) {
            message_obj.insert("reasoning_content".into(), serde_json::Value::String(rc.clone()));
        }
        if let Some(tcs) = msg.and_then(|m| m.tool_calls.as_ref()) {
            message_obj.insert("tool_calls".into(), serde_json::to_value(tcs).unwrap_or_default());
        }
        if let Some(tcid) = msg.and_then(|m| m.tool_call_id.clone()) {
            message_obj.insert("tool_call_id".into(), serde_json::Value::String(tcid));
        }
        serde_json::json!({
            "index": c.index,
            "message": message_obj,
            "finish_reason": c.finish_reason,
        })
    }).collect();

    serde_json::json!({
        "id": unified.id,
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": unified.model,
        "choices": choices,
        "usage": unified.usage.as_ref().map(|u| serde_json::json!({
            "prompt_tokens": u.prompt_tokens,
            "completion_tokens": u.completion_tokens,
            "total_tokens": u.total_tokens,
        })),
    })
}

/// 生成虚拟模型的 OpenAI models 列表（含非标准能力扩展，能读的客户端会受益）
pub fn models_list(models: &[ModelInfo]) -> serde_json::Value {
    let data: Vec<serde_json::Value> = models.iter().map(|m| {
        let reasoning = m.capabilities.iter().any(|c| c.eq_ignore_ascii_case("Reasoning"));
        let tool = m.capabilities.iter().any(|c| c.eq_ignore_ascii_case("Tool"));
        let mut item = serde_json::json!({
            "id": m.name, "object": "model", "created": 0, "owned_by": "ai-gateway"
        });
        if !m.capabilities.is_empty() {
            item["capabilities"] = serde_json::json!(m.capabilities);
        }
        if reasoning {
            item["supported_parameters"] = serde_json::json!([
                "tools", "tool_choice", "reasoning_effort", "reasoning", "thinking", "max_completion_tokens"
            ]);
            item["reasoning"] = serde_json::json!({
                "efforts": ["minimal", "low", "medium", "high", "xhigh"],
                "budget_tokens": true
            });
        } else if tool {
            item["supported_parameters"] = serde_json::json!(["tools", "tool_choice"]);
        }
        item
    }).collect();
    serde_json::json!({ "object": "list", "data": data })
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub capabilities: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::route::ReasoningEcho;

    fn target(style: ReasoningStyle, echo: ReasoningEcho) -> ReasoningTarget {
        ReasoningTarget::new(style, "b1", "m1", echo)
    }

    fn unified(raw: serde_json::Value) -> UnifiedRequest {
        parse_request(raw).unwrap()
    }

    #[test]
    fn vendor_styles_emit_expected_fields() {
        let raw = serde_json::json!({
            "model": "virt", "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 32768, "reasoning_effort": "high"
        });

        let qwen = to_request(&unified(raw.clone()), "m", UnknownParams::Auto, &target(ReasoningStyle::Qwen, ReasoningEcho::Drop));
        assert_eq!(qwen["enable_thinking"], true);
        assert_eq!(qwen["thinking_budget"], 16384);

        let glm = to_request(&unified(raw.clone()), "m", UnknownParams::Auto, &target(ReasoningStyle::Glm, ReasoningEcho::Drop));
        assert_eq!(glm["thinking"]["type"], "enabled");
        assert!(glm["thinking"].get("budget_tokens").is_none());

        let openrouter = to_request(&unified(raw.clone()), "m", UnknownParams::Auto, &target(ReasoningStyle::OpenRouter, ReasoningEcho::Drop));
        assert_eq!(openrouter["reasoning"]["effort"], "high");
        assert_eq!(openrouter["reasoning"]["max_tokens"], 16384);

        let vllm = to_request(&unified(raw), "m", UnknownParams::Auto, &target(ReasoningStyle::VllmQwen, ReasoningEcho::Drop));
        assert_eq!(vllm["chat_template_kwargs"]["enable_thinking"], true);
    }

    #[test]
    fn no_reasoning_input_emits_no_vendor_switch() {
        let raw = serde_json::json!({
            "model": "virt", "messages": [{"role": "user", "content": "hi"}]
        });
        let qwen = to_request(&unified(raw), "m", UnknownParams::Auto, &target(ReasoningStyle::Qwen, ReasoningEcho::Drop));
        assert!(qwen.get("enable_thinking").is_none());
    }

    #[test]
    fn explicit_disable_emits_false_for_qwen() {
        let raw = serde_json::json!({
            "model": "virt", "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "none"
        });
        let qwen = to_request(&unified(raw), "m", UnknownParams::Auto, &target(ReasoningStyle::Qwen, ReasoningEcho::Drop));
        assert_eq!(qwen["enable_thinking"], false);
    }

    #[test]
    fn max_completion_tokens_alias_is_preserved() {
        let raw = serde_json::json!({
            "model": "virt", "messages": [{"role": "user", "content": "hi"}],
            "max_completion_tokens": 123
        });
        let req = unified(raw);
        assert_eq!(req.max_tokens, Some(123));
        let body = to_request(&req, "m", UnknownParams::Auto, &target(ReasoningStyle::OpenAiEffort, ReasoningEcho::Drop));
        assert_eq!(body["max_completion_tokens"], 123);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn models_list_advertises_reasoning() {
        let models = vec![ModelInfo {
            name: "virt".into(),
            capabilities: vec!["Reasoning".into(), "Tool".into()],
        }];
        let v = models_list(&models);
        assert_eq!(v["data"][0]["capabilities"][0], "Reasoning");
        assert!(v["data"][0]["supported_parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "reasoning_effort"));
        assert_eq!(v["data"][0]["reasoning"]["budget_tokens"], true);
    }

    #[test]
    fn reasoning_echo_is_configurable() {
        let raw = serde_json::json!({
            "model": "virt",
            "messages": [
                {"role": "assistant", "content": "answer", "reasoning_content": "thought"},
                {"role": "user", "content": "next"}
            ]
        });
        let req = unified(raw);
        let keep = to_request(&req, "m", UnknownParams::Auto, &target(ReasoningStyle::OpenAiEffort, ReasoningEcho::Keep));
        assert_eq!(keep["messages"][0]["reasoning_content"], "thought");
        let drop = to_request(&req, "m", UnknownParams::Auto, &target(ReasoningStyle::OpenAiEffort, ReasoningEcho::Drop));
        assert!(drop["messages"][0].get("reasoning_content").is_none());
    }
}
