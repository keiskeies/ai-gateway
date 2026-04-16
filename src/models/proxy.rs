use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum Protocol {
    OpenAI,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ProxyStatus {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub id: String,
    pub name: String,
    pub listen_port: u16,
    pub protocols: Vec<Protocol>,
    pub auth_token: Option<String>,
    pub status: ProxyStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProxyRequest {
    pub name: String,
    pub listen_port: u16,
    #[serde(default = "default_protocols")]
    pub protocols: Vec<Protocol>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProxyRequest {
    pub name: Option<String>,
    pub listen_port: Option<u16>,
    pub protocols: Option<Vec<Protocol>>,
    pub auth_token: Option<String>,
}

fn default_protocols() -> Vec<Protocol> {
    vec![Protocol::OpenAI]
}
