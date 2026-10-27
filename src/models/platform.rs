use serde::{Deserialize, Serialize, Serializer};

use super::api_key::NullableString;
use super::model::NullableNumber;

/// 积分窗口只允许这三个自然窗口。与 Key 配额的滚动窗口不同：
/// 上游额度通常在本地零点重置，所以这里按自然日/周/月计算。
pub const CREDIT_WINDOWS: [&str; 3] = ["day", "week", "month"];

/// 校验额度单位与预算/窗口的组合是否自洽。
///
/// `Credits` 必须给预算与窗口（否则不知道什么时候重置）；
/// `Currency` / `Requests` / `None` 不该带预算——带了也不会生效，只会误导用户。
pub fn validate_credit_unit(
    unit: Option<&str>,
    budget: Option<f64>,
    window: Option<&str>,
) -> Result<Option<String>, String> {
    let resolved = super::adapter::CreditUnit::resolve(unit);
    match resolved {
        super::adapter::CreditUnit::Credits => {
            if budget.is_none() {
                return Err("credit_unit=credits requires credit_budget".to_string());
            }
            validate_credit_fields(budget, window)
        }
        _ => {
            if budget.is_some() {
                return Err(format!(
                    "credit_unit={} 不使用积分台账，请清空 credit_budget",
                    resolved.key_str()
                ));
            }
            Ok(None)
        }
    }
}

/// 预算与窗口必须同时存在：有预算没窗口就不知道什么时候重置，
/// 有窗口没预算则这条配置不产生任何效果。调用方把用户输入归一后再落库。
pub fn validate_credit_fields(budget: Option<f64>, window: Option<&str>) -> Result<Option<String>, String> {
    match budget {
        Some(value) => {
            if !value.is_finite() || value <= 0.0 {
                return Err("credit_budget must be greater than 0".to_string());
            }
            let raw = window.unwrap_or("day").trim().to_ascii_lowercase();
            if !CREDIT_WINDOWS.contains(&raw.as_str()) {
                return Err("credit_window must be day, week or month".to_string());
            }
            Ok(Some(raw))
        }
        None => {
            if window.is_some() {
                return Err("credit_window requires credit_budget".to_string());
            }
            Ok(None)
        }
    }
}

fn keep_number() -> NullableNumber { NullableNumber::Keep }
fn keep_string() -> NullableString { NullableString::Keep }

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
    /// 上游账号的积分预算（如魔塔每天 250 分）。NULL = 不启用积分计费。
    /// 只有在 `credit_unit = Credits` 时才真正走台账。
    pub credit_budget: Option<f64>,
    /// 自然窗口：day / week / month；NULL = 不启用。
    pub credit_window: Option<String>,
    /// 额度计量单位。NULL = None（不计量）。见 `CreditUnit`。
    pub credit_unit: Option<String>,
    /// 请求构造方式。NULL = 由 `platform_type` 推出（兼容 V28 之前的行）。
    pub adapter: Option<String>,
    /// 上游目录键：type + 规范化 base_url 的 SHA-256，不含 API Key。
    /// 同一 key 的账号共享 models 目录与价格。
    #[serde(default)]
    pub catalog_key: Option<String>,
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
    #[serde(default)]
    pub credit_budget: Option<f64>,
    #[serde(default)]
    pub credit_window: Option<String>,
    /// 额度计量单位：none / credits / currency / requests。默认 none。
    #[serde(default)]
    pub credit_unit: Option<String>,
    /// 请求构造方式：OpenAI / Anthropic / Trae / WorkBuddy。默认由 type 推出。
    #[serde(default)]
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePlatformRequest {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub platform_type: Option<PlatformType>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub organization: Option<String>,
    pub custom_headers: Option<serde_json::Value>,
    pub status: Option<PlatformStatus>,
    /// 三态：没写 = 不改，null = 关闭积分，数字 = 设值。
    #[serde(default = "keep_number")]
    pub credit_budget: NullableNumber,
    /// 三态：没写 = 不改，null = 关闭积分，字符串 = day/week/month。
    #[serde(default = "keep_string")]
    pub credit_window: NullableString,
    /// 三态：没写 = 不改，null = 清空（回落 none），字符串 = none/credits/currency/requests。
    #[serde(default = "keep_string")]
    pub credit_unit: NullableString,
    /// 三态：没写 = 不改，null = 清空（回落由 type 推出），字符串 = 具体 adapter。
    #[serde(default = "keep_string")]
    pub adapter: NullableString,
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
    /// 预设建议的 adapter；NULL = 由 platform_type 推出
    #[serde(default)]
    pub adapter: Option<String>,
    /// 预设建议的额度单位；NULL = none
    #[serde(default)]
    pub credit_unit: Option<String>,
}

impl PlatformPreset {
    fn plain(name: &str, platform_type: PlatformType, base_url: &str) -> Self {
        Self {
            name: name.into(),
            platform_type,
            base_url: base_url.into(),
            adapter: None,
            credit_unit: None,
        }
    }

    fn with(name: &str, base_url: &str, adapter: &str, credit_unit: &str) -> Self {
        Self {
            name: name.into(),
            platform_type: PlatformType::OpenAI,
            base_url: base_url.into(),
            adapter: Some(adapter.into()),
            credit_unit: Some(credit_unit.into()),
        }
    }

    pub fn all() -> Vec<Self> {
        use PlatformType::*;
        vec![
            // —— 私有协议上游（adapter 决定请求怎么构造） ——
            Self::with("Trae（国内）", "https://trae-api-cn.mchost.guru", "Trae", "requests"),
            Self::with("Trae（海外）", "https://core-normal.traeapi.us", "Trae", "currency"),
            Self::with("WorkBuddy（国内）", "https://copilot.tencent.com", "WorkBuddy", "credits"),
            Self::with("WorkBuddy（海外）", "https://www.workbuddy.ai", "WorkBuddy", "credits"),
            // —— 标准 OpenAI 兼容 ——
            // Agnes 国内站是 Credits（1000 = ¥1），本质是钱 → currency；海外站是请求次数 → requests
            Self::with("Agnes AI（国内）", "https://api.agnes-ai.cn/v1", "OpenAI", "currency"),
            Self::with("Agnes AI（海外）", "https://apihub.agnes-ai.com/v1", "OpenAI", "requests"),
            Self::with("ModelScope 魔搭", "https://api-inference.modelscope.cn/v1", "OpenAI", "requests"),
            Self::plain("OpenAI", OpenAI, "https://api.openai.com/v1"),
            Self::plain("Anthropic", Anthropic, "https://api.anthropic.com"),
            Self::plain("Ollama", Ollama, "http://localhost:11434/v1"),
            Self::plain("Azure OpenAI", Azure, ""),
            Self::plain("NVIDIA", OpenAI, "https://integrate.api.nvidia.com/v1"),
            Self::plain("DeepSeek", OpenAI, "https://api.deepseek.com/v1"),
            Self::plain("Moonshot", OpenAI, "https://api.moonshot.cn/v1"),
            Self::plain("ZhipuAI", OpenAI, "https://open.bigmodel.cn/api/paas/v4"),
            Self::plain("Qwen", OpenAI, "https://dashscope.aliyuncs.com/compatible-mode/v1"),
            Self::plain("Doubao", OpenAI, "https://ark.cn-beijing.volces.com/api/v3"),
            Self::plain("SiliconFlow", OpenAI, "https://api.siliconflow.cn/v1"),
            Self::plain("Groq", OpenAI, "https://api.groq.com/openai/v1"),
            Self::plain("Google Gemini", OpenAI, "https://generativelanguage.googleapis.com/v1beta/openai"),
            Self::plain("Together AI", OpenAI, "https://api.together.xyz/v1"),
            Self::plain("OpenRouter", OpenAI, "https://openrouter.ai/api/v1"),
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
            credit_budget: None,
            credit_window: None,
            credit_unit: None,
            adapter: None,
            catalog_key: None,
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

    #[test]
    fn credits_unit_requires_budget_and_window() {
        // credits 必须给预算，否则不知道什么时候重置
        assert!(validate_credit_unit(Some("Credits"), None, None).is_err());
        // 给了预算没给窗口时，validate_credit_fields 默认补 day
        assert_eq!(
            validate_credit_unit(Some("Credits"), Some(100.0), None).unwrap(),
            Some("day".to_string())
        );
        assert_eq!(
            validate_credit_unit(Some("Credits"), Some(100.0), Some("month")).unwrap(),
            Some("month".to_string())
        );
    }

    #[test]
    fn non_credits_units_reject_budget() {
        // currency / requests / none 不走台账，带预算只会误导用户
        for unit in ["Currency", "Requests", "None"] {
            assert!(
                validate_credit_unit(Some(unit), Some(100.0), None).is_err(),
                "{unit} 不该接受 credit_budget"
            );
            assert!(validate_credit_unit(Some(unit), None, None).is_ok());
        }
        // 缺省（NULL）等同 None
        assert!(validate_credit_unit(None, Some(100.0), None).is_err());
        assert!(validate_credit_unit(None, None, None).is_ok());
    }

    #[test]
    fn presets_declare_adapter_and_unit_for_proprietary_upstreams() {
        let all = PlatformPreset::all();
        for name in [
            "Trae（国内）",
            "Trae（海外）",
            "WorkBuddy（国内）",
            "WorkBuddy（海外）",
        ] {
            let p = all.iter().find(|p| p.name == name).expect(name);
            assert!(
                p.adapter.as_deref().map(|a| a == "Trae" || a == "WorkBuddy").unwrap_or(false),
                "{name} 必须声明私有 adapter"
            );
        }
        // 标准上游不该被误标 adapter
        let openai = all.iter().find(|p| p.name == "OpenAI").unwrap();
        assert!(openai.adapter.is_none());
        assert!(openai.credit_unit.is_none());
    }

    #[test]
    fn every_preset_name_is_unique() {
        let all = PlatformPreset::all();
        let mut names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(total, names.len(), "预设名不能重复（前端用名字做 Select 的 value）");
    }
}
