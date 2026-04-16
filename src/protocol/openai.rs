use serde::{Deserialize, Serialize};
use super::types::*;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
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
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
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
                system = Some(msg.content.as_ref()
                    .and_then(|c| c.as_str()).unwrap_or("").to_string());
                // 也添加到 messages 中保留
                messages.push(UnifiedMessage {
                    role: Role::System,
                    content: msg.content.as_ref().and_then(|c| c.as_str()).map(|s| s.to_string()),
                    tool_calls: None, tool_call_id: None, name: None,
                });
            }
            "user" => messages.push(UnifiedMessage {
                role: Role::User,
                content: msg.content.as_ref().and_then(|c| c.as_str()).map(|s| s.to_string()),
                tool_calls: None, tool_call_id: None, name: None,
            }),
            "assistant" => messages.push(UnifiedMessage {
                role: Role::Assistant,
                content: msg.content.as_ref().and_then(|c| c.as_str()).map(|s| s.to_string()),
                tool_calls: msg.tool_calls.map(|tcs| tcs.into_iter().map(|tc| UnifiedToolCall {
                    id: tc.id, call_type: tc.call_type,
                    function: UnifiedFunctionCall { name: tc.function.name, arguments: tc.function.arguments },
                }).collect()),
                tool_call_id: None, name: None,
            }),
            "tool" => messages.push(UnifiedMessage {
                role: Role::Tool,
                content: msg.content.as_ref().and_then(|c| c.as_str()).map(|s| s.to_string()),
                tool_calls: None, tool_call_id: msg.tool_call_id, name: msg.name,
            }),
            _ => {}
        }
    }

    let stop = req.stop.and_then(|s| {
        if let Some(arr) = s.as_array() { Some(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()) } else { s.as_str().map(|s| vec![s.to_string()]) }
    });

    Ok(UnifiedRequest {
        model: req.model, messages, system,
        max_tokens: req.max_tokens, temperature: req.temperature, top_p: req.top_p,
        stream: req.stream, stop,
        tools: req.tools.map(|tools| tools.into_iter().map(|t| UnifiedTool {
            id: format!("tool_{}", t.function.name), tool_type: t.tool_type,
            function: UnifiedFunction { name: t.function.name, description: t.function.description, parameters: t.function.parameters },
        }).collect()),
        tool_choice: None,
    })
}

/// 统一格式 → OpenAI 请求
pub fn to_request(unified: &UnifiedRequest, target_model: &str) -> serde_json::Value {
    let messages: Vec<OpenAIMessage> = unified.messages.iter().map(|m| {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        OpenAIMessage {
            role: role.to_string(),
            content: m.content.as_ref().map(|c| serde_json::Value::String(c.clone())),
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
    if let Some(v) = unified.max_tokens { req.insert("max_tokens".into(), serde_json::to_value(v).unwrap()); }
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
    serde_json::Value::Object(req)
}

/// OpenAI 响应 → 统一格式
pub fn parse_response(raw: serde_json::Value) -> Result<UnifiedResponse, String> {
    let resp: OpenAIChatResponse = serde_json::from_value(raw)
        .map_err(|e| format!("Invalid OpenAI response: {}", e))?;

    Ok(UnifiedResponse {
        id: resp.id, model: resp.model,
        choices: resp.choices.into_iter().map(|c| {
            UnifiedChoice {
                index: c.index,
                message: Some(UnifiedMessage {
                    role: match c.message.role.as_str() {
                        "assistant" => Role::Assistant, "user" => Role::User,
                        "system" => Role::System, _ => Role::Assistant,
                    },
                    content: c.message.content.as_ref().and_then(|c| c.as_str()).map(|s| s.to_string()),
                    tool_calls: c.message.tool_calls.map(|tcs| tcs.into_iter().map(|tc| UnifiedToolCall {
                        id: tc.id, call_type: tc.call_type,
                        function: UnifiedFunctionCall { name: tc.function.name, arguments: tc.function.arguments },
                    }).collect()),
                    tool_call_id: None, name: None,
                }),
                finish_reason: c.finish_reason,
            }
        }).collect(),
        usage: resp.usage.map(|u| UnifiedUsage { prompt_tokens: u.prompt_tokens, completion_tokens: u.completion_tokens, total_tokens: u.total_tokens }),
    })
}

/// 统一格式 → OpenAI 响应
pub fn to_response(unified: &UnifiedResponse) -> serde_json::Value {
    let resp = OpenAIChatResponse {
        id: unified.id.clone(), object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp(), model: unified.model.clone(),
        choices: unified.choices.iter().map(|c| {
            let msg = c.message.as_ref();
            OpenAIChoice {
                index: c.index,
                message: OpenAIMessage {
                    role: msg.map(|m| match m.role { Role::Assistant => "assistant", Role::User => "user", Role::System => "system", Role::Tool => "tool" }).unwrap_or("assistant").to_string(),
                    content: msg.and_then(|m| m.content.as_ref()).map(|c| serde_json::Value::String(c.clone())),
                    tool_calls: msg.and_then(|m| m.tool_calls.as_ref()).map(|tcs| tcs.iter().map(|tc| OpenAIToolCall {
                        id: tc.id.clone(), call_type: tc.call_type.clone(),
                        function: OpenAIFunctionCall { name: tc.function.name.clone(), arguments: tc.function.arguments.clone() },
                    }).collect()),
                    tool_call_id: msg.and_then(|m| m.tool_call_id.clone()), name: msg.and_then(|m| m.name.clone()),
                },
                finish_reason: c.finish_reason.clone(),
            }
        }).collect(),
        usage: unified.usage.as_ref().map(|u| OpenAIUsage { prompt_tokens: u.prompt_tokens, completion_tokens: u.completion_tokens, total_tokens: u.total_tokens }),
    };
    serde_json::to_value(resp).unwrap_or_default()
}

/// 生成虚拟模型的 OpenAI models 列表
pub fn models_list(virtual_models: &[String]) -> serde_json::Value {
    serde_json::json!({
        "object": "list",
        "data": virtual_models.iter().map(|m| serde_json::json!({
            "id": m, "object": "model", "created": 0, "owned_by": "ai-gateway"
        })).collect::<Vec<_>>()
    })
}
