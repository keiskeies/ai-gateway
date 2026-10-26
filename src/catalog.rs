use sha2::{Digest, Sha256};

use crate::models::platform::PlatformType;

/// 只归一化 scheme/host 与末尾斜杠：path 可能大小写敏感，不能整体小写。
pub fn normalize_base_url(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    if let Some(scheme_end) = value.find("://") {
        let scheme = value[..scheme_end].to_ascii_lowercase();
        let rest = &value[scheme_end + 3..];
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        value = format!("{}://{}{}", scheme, authority.to_ascii_lowercase(), path);
    }
    while value.ends_with('/') {
        value.pop();
    }
    value
}

fn type_key(platform_type: &PlatformType) -> &'static str {
    match platform_type {
        PlatformType::OpenAI => "openai",
        PlatformType::Anthropic => "anthropic",
        PlatformType::Ollama => "ollama",
        PlatformType::Azure => "azure",
        PlatformType::Custom => "custom",
    }
}

/// 同一上游（类型 + 规范化 URL）的账号共享模型目录；API Key 不参与。
pub fn catalog_key(platform_type: &PlatformType, base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    let mut hasher = Sha256::new();
    hasher.update(type_key(platform_type).as_bytes());
    hasher.update(b"|");
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_url_ignores_case_and_trailing_slash() {
        let a = catalog_key(&PlatformType::OpenAI, "https://API.DeepSeek.com/v1/");
        let b = catalog_key(&PlatformType::OpenAI, "  https://api.deepseek.com/v1  ");
        assert_eq!(a, b);
        assert_eq!(normalize_base_url("https://API.DeepSeek.com/v1//"), "https://api.deepseek.com/v1");
    }

    #[test]
    fn type_and_path_still_matter() {
        let openai = catalog_key(&PlatformType::OpenAI, "https://api.deepseek.com/v1");
        let anthropic = catalog_key(&PlatformType::Anthropic, "https://api.deepseek.com/v1");
        let v2 = catalog_key(&PlatformType::OpenAI, "https://api.deepseek.com/v2");
        assert_ne!(openai, anthropic);
        assert_ne!(openai, v2);
        assert_eq!(openai.len(), 64);
    }
}
