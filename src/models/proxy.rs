use serde::{Deserialize, Serialize};

/// Proxy = Virtual Model. Name is the model ID exposed to clients.
/// All proxies support both OpenAI and Anthropic protocols by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub id: String,
    pub name: String,
    /// Capabilities computed as intersection of all backend model capabilities
    #[serde(default)]
    pub capabilities: Vec<crate::models::model::Capability>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProxyRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProxyRequest {
    pub name: Option<String>,
}

/// Internal enum for response protocol selection (not stored in DB)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Protocol {
    OpenAI,
    Anthropic,
}
