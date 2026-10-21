use serde::{Deserialize, Serialize, Serializer};

/// 平台密钥只在进程内和转发时使用，管理接口的响应一律返回掩码：
/// 明文既不出浏览器，也不可能被表单"原样保存"回去。
pub fn mask_secret(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= 8 {
        return "••••••••".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}••••••••{}", head, tail)
}

fn serialize_masked<S: Serializer>(value: &String, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&mask_secret(value))
}

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
    #[serde(serialize_with = "serialize_masked")]
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

/// 保存前测试连接用的请求：字段都可以是"还没写进数据库"的表单值。
/// `platform_id` 用于编辑场景——表单里 Key 留空时取回库里已存的 Key，Key 本身永不回传。
/// `custom_headers` 与转发热路径同口径（鉴权头之后附加，可覆盖），否则靠自定义头鉴权的网关测不出真实结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformTestRequest {
    #[serde(rename = "type", default = "default_platform_type")]
    pub platform_type: PlatformType,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub platform_id: Option<String>,
    #[serde(default)]
    pub custom_headers: Option<serde_json::Value>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(api_key: &str) -> Platform {
        Platform {
            id: "p1".into(),
            name: "demo".into(),
            platform_type: PlatformType::OpenAI,
            base_url: "https://api.example.com/v1".into(),
            api_key: api_key.into(),
            organization: None,
            custom_headers: serde_json::json!({}),
            status: PlatformStatus::Active,
            rate_limit: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn api_key_is_serialized_masked() {
        let json = serde_json::to_value(sample("sk-secret-abcdef123456")).unwrap();
        assert_eq!(json["api_key"], "sk-s••••••••3456");
        assert!(!json.to_string().contains("abcdef12"));
    }

    #[test]
    fn short_and_empty_keys_stay_unusable() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("abc123"), "••••••••");
    }

    #[test]
    fn masked_value_is_not_the_stored_credential() {
        // 序列化只用于出网：内部结构仍持有明文，转发链路依赖它
        assert_eq!(sample("sk-secret-abcdef123456").api_key, "sk-secret-abcdef123456");
    }
}
