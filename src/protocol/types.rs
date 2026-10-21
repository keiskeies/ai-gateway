use serde::{Deserialize, Serialize};

use super::reasoning::{MaxTokensField, ReasoningConfig};

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
    /// 客户端入口协议，用于决定未知参数策略和同协议回放
    #[serde(default)]
    pub source_protocol: crate::models::proxy::Protocol,
    /// 归一化后的思考配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    /// 客户端原始 max_tokens 字段名
    #[serde(default)]
    pub max_tokens_field: MaxTokensField,
    /// 保留原始请求中的未知字段，确保端到端透传不丢失
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub role: Role,
    /// OpenAI 风格内容：字符串或内容块数组（多模态），见 `protocol::content`
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<UnifiedToolCall>>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    /// Anthropic thinking / redacted_thinking 原始块（含 signature / data）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_blocks: Vec<serde_json::Value>,
    /// Anthropic 入口的完整 assistant content 原始块序列，同协议时原样回放
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_blocks: Vec<serde_json::Value>,
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
    /// 命中上游提示词缓存的 token 数（OpenAI cached_tokens / Anthropic cache_read_input_tokens）
    #[serde(default)]
    pub cache_read_tokens: Option<u32>,
    /// 缓存写入 token 数（目前仅 Anthropic cache_creation_input_tokens）
    #[serde(default)]
    pub cache_write_tokens: Option<u32>,
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
