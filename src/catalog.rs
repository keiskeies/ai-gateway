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
    catalog_key_with_adapter(platform_type, base_url, None)
}

/// 带 adapter 的目录键。
///
/// **只有私有协议 adapter（Trae / WorkBuddy）才参与哈希**，这是刻意的：
///
/// - 私有协议的模型目录来自各自的私有接口，与同域名的 OpenAI 兼容目录完全不是
///   一回事；若 base_url 恰好相同，不区分就会把两套模型列表和价格混成一份。
/// - 但 `OpenAI` / `Anthropic` 这两个 adapter 与 `platform_type` 一一对应，
///   把它们也塞进哈希会让**所有存量行的 catalog_key 全部变化**，升级后模型目录
///   会“凭空消失”（旧 key 的模型行变成孤儿被清掉）。所以标准 adapter 保持
///   V25 的原始算法不变。
pub fn catalog_key_with_adapter(
    platform_type: &PlatformType,
    base_url: &str,
    adapter: Option<&str>,
) -> String {
    let normalized = normalize_base_url(base_url);
    let mut hasher = Sha256::new();
    hasher.update(type_key(platform_type).as_bytes());
    // 仅私有协议追加 adapter，标准协议不追加（保证存量 key 稳定）
    if let Some(extra) = adapter.and_then(proprietary_key) {
        hasher.update(b":");
        hasher.update(extra.as_bytes());
    }
    hasher.update(b"|");
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

/// adapter 字符串 → 参与哈希的标识。标准 adapter 返回 None（不参与）。
fn proprietary_key(adapter: &str) -> Option<&'static str> {
    match adapter.trim() {
        "Trae" => Some("trae"),
        "WorkBuddy" => Some("workbuddy"),
        _ => None,
    }
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

    #[test]
    fn standard_adapter_does_not_change_legacy_key() {
        // 关键回归：V28 升级后存量平台的 catalog_key 必须一字不变，
        // 否则旧模型行会变成孤儿被 delete_orphan_catalogs 清掉。
        let legacy = catalog_key(&PlatformType::OpenAI, "https://api.deepseek.com/v1");
        assert_eq!(
            legacy,
            catalog_key_with_adapter(&PlatformType::OpenAI, "https://api.deepseek.com/v1", Some("OpenAI"))
        );
        assert_eq!(
            catalog_key(&PlatformType::Anthropic, "https://api.anthropic.com"),
            catalog_key_with_adapter(&PlatformType::Anthropic, "https://api.anthropic.com", Some("Anthropic"))
        );
        // 未知/脏 adapter 也不改变哈希
        assert_eq!(
            legacy,
            catalog_key_with_adapter(&PlatformType::OpenAI, "https://api.deepseek.com/v1", Some("garbage"))
        );
    }

    #[test]
    fn proprietary_adapters_are_separated_from_standard() {
        // Trae 与 WorkBuddy 若恰好共用 base_url，目录必须分开
        let base = "https://shared.example.com";
        let openai = catalog_key_with_adapter(&PlatformType::OpenAI, base, Some("OpenAI"));
        let trae = catalog_key_with_adapter(&PlatformType::OpenAI, base, Some("Trae"));
        let workbuddy = catalog_key_with_adapter(&PlatformType::OpenAI, base, Some("WorkBuddy"));
        assert_ne!(openai, trae);
        assert_ne!(openai, workbuddy);
        assert_ne!(trae, workbuddy);
    }

    #[test]
    fn same_proprietary_adapter_still_shares_catalog() {
        // 同一 adapter + 同一 URL 的多个账号必须共享目录（多账号池的收益所在）
        let a = catalog_key_with_adapter(&PlatformType::OpenAI, "https://api.trae.com.cn/", Some("Trae"));
        let b = catalog_key_with_adapter(&PlatformType::OpenAI, "https://API.TRAE.COM.CN", Some("Trae"));
        assert_eq!(a, b);
    }
}
