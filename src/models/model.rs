use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ModelStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum Capability {
    Chat,
    Completion,
    Embedding,
    Vision,
    FunctionCall,
    Streaming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub platform_id: String,
    pub model_id: String,
    pub display_name: String,
    pub max_tokens: u32,
    pub context_window: u32,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub capabilities: Vec<Capability>,
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelRequest {
    pub platform_id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelRequest {
    pub display_name: Option<String>,
    pub max_tokens: Option<u32>,
    pub context_window: Option<u32>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub capabilities: Option<Vec<Capability>>,
    pub status: Option<ModelStatus>,
}

fn default_max_tokens() -> u32 { 4096 }
fn default_context_window() -> u32 { 8192 }
