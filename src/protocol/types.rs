use serde::{Deserialize, Serialize};

/// 统一内部消息格式 - 所有协议转换的中间表示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub model: String,
    pub messages: Vec<UnifiedMessage>,
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub tools: Option<Vec<UnifiedTool>>,
    pub tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub role: Role,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<UnifiedToolCall>>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedTool {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: UnifiedFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: UnifiedFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    None(String),
    Object { r#type: String, function: Option<ToolFunction> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
}

/// 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<UnifiedChoice>,
    pub usage: Option<UnifiedUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChoice {
    pub index: u32,
    pub message: Option<UnifiedMessage>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 统一流式chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedStreamChunk {
    pub id: String,
    pub model: String,
    pub choices: Vec<UnifiedStreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedStreamChoice {
    pub index: u32,
    pub delta: Option<UnifiedMessage>,
    pub finish_reason: Option<String>,
}
