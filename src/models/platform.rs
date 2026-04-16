use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PlatformType {
    OpenAI,
    Anthropic,
    Ollama,
    Azure,
    Custom,
}

impl PlatformType {
    pub fn all() -> Vec<Self> {
        vec![Self::OpenAI, Self::Anthropic, Self::Ollama, Self::Azure, Self::Custom]
    }

    pub fn label(&self) -> &str {
        match self {
            Self::OpenAI => "OpenAI Compatible",
            Self::Anthropic => "Anthropic",
            Self::Ollama => "Ollama",
            Self::Azure => "Azure OpenAI",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PlatformStatus {
    Active,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub platform_type: PlatformType,
    pub base_url: String,
    pub api_key: String,
    pub organization: Option<String>,
    pub custom_headers: serde_json::Value,
    pub status: PlatformStatus,
    pub rate_limit: Option<RateLimit>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub rpm: Option<u32>,  // requests per minute
    pub tpm: Option<u32>,  // tokens per minute
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlatformRequest {
    pub name: String,
    #[serde(rename = "type", default = "default_platform_type")]
    pub platform_type: PlatformType,
    pub base_url: String,
    pub api_key: String,
    pub organization: Option<String>,
    #[serde(default)]
    pub custom_headers: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlatformRequest {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub platform_type: Option<PlatformType>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub organization: Option<String>,
    pub custom_headers: Option<serde_json::Value>,
    pub status: Option<PlatformStatus>,
}

fn default_platform_type() -> PlatformType {
    PlatformType::OpenAI
}

/// 内置平台预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPreset {
    pub name: String,
    pub platform_type: PlatformType,
    pub base_url: String,
}

impl PlatformPreset {
    pub fn all() -> Vec<Self> {
        vec![
            Self { name: "OpenAI".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.openai.com/v1".into() },
            Self { name: "Anthropic".into(), platform_type: PlatformType::Anthropic, base_url: "https://api.anthropic.com".into() },
            Self { name: "Ollama".into(), platform_type: PlatformType::Ollama, base_url: "http://localhost:11434/v1".into() },
            Self { name: "Azure OpenAI".into(), platform_type: PlatformType::Azure, base_url: String::new() },
            Self { name: "NVIDIA".into(), platform_type: PlatformType::OpenAI, base_url: "https://integrate.api.nvidia.com/v1".into() },
            Self { name: "DeepSeek".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.deepseek.com/v1".into() },
            Self { name: "Moonshot".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.moonshot.cn/v1".into() },
            Self { name: "ZhipuAI".into(), platform_type: PlatformType::OpenAI, base_url: "https://open.bigmodel.cn/api/paas/v4".into() },
            Self { name: "Qwen".into(), platform_type: PlatformType::OpenAI, base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into() },
            Self { name: "Doubao".into(), platform_type: PlatformType::OpenAI, base_url: "https://ark.cn-beijing.volces.com/api/v3".into() },
            Self { name: "SiliconFlow".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.siliconflow.cn/v1".into() },
            Self { name: "Groq".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.groq.com/openai/v1".into() },
            Self { name: "Google Gemini".into(), platform_type: PlatformType::OpenAI, base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into() },
            Self { name: "Together AI".into(), platform_type: PlatformType::OpenAI, base_url: "https://api.together.xyz/v1".into() },
            Self { name: "OpenRouter".into(), platform_type: PlatformType::OpenAI, base_url: "https://openrouter.ai/api/v1".into() },
        ]
    }
}
