use serde::{Deserialize, Serialize};

use super::platform::PlatformType;

/// 上游「请求怎么构造」的方式。
///
/// 与 `PlatformType` 正交：`platform_type` 表达上游的**响应协议族**（决定响应怎么解析、
/// 是否跨协议转换），`adapter` 表达**请求怎么构造**。两者默认一致，所以既有的
/// 15 个预设零改动。
///
/// 为什么用枚举而不是 trait：项目现有风格是枚举 + match（`PlatformType`、
/// `ReasoningStyle`、`Protocol` 都是），没有 trait object。四个变体、两处 match，
/// 引入 trait 会带来对象安全 / 生命周期 / Send 约束的额外复杂度，收益不大。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlatformAdapter {
    /// 标准 OpenAI Chat Completions（含绝大多数兼容上游）
    OpenAI,
    /// 标准 Anthropic Messages
    Anthropic,
    /// Trae 私有协议（Cloud-IDE-JWT + llm_utils_chat / chat_sessions）
    Trae,
    /// WorkBuddy / CodeBuddy 私有协议（Bearer + /v2/chat/completions）
    WorkBuddy,
}

impl PlatformAdapter {
    pub fn all() -> Vec<Self> {
        vec![Self::OpenAI, Self::Anthropic, Self::Trae, Self::WorkBuddy]
    }

    /// 旧行没有 `adapter` 列，由 `platform_type` 推出默认值。
    ///
    /// 解析必须**大小写不敏感**：`key_str()` 写入的是小写（"trae"），
    /// 而 serde 的 `rename_all = "PascalCase"` 只认 "Trae"。两者不一致会让
    /// 「存进去再读出来」变成默认值，私有协议静默退化成 OpenAI 协议。
    pub fn resolve(stored: Option<&str>, platform_type: &PlatformType) -> Self {
        match stored.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => match raw.to_ascii_lowercase().as_str() {
                "openai" => Self::OpenAI,
                "anthropic" => Self::Anthropic,
                "trae" => Self::Trae,
                "workbuddy" => Self::WorkBuddy,
                _ => Self::from_type(platform_type),
            },
            None => Self::from_type(platform_type),
        }
    }

    /// 从 `platform_type` 推出默认 adapter：只有 Anthropic 有专属协议，其余都走 OpenAI。
    pub fn from_type(platform_type: &PlatformType) -> Self {
        match platform_type {
            PlatformType::Anthropic => Self::Anthropic,
            _ => Self::OpenAI,
        }
    }

    /// 是否走私有协议。决定是否需要凭据保险箱、是否需要请求体改写。
    pub fn is_proprietary(&self) -> bool {
        matches!(self, Self::Trae | Self::WorkBuddy)
    }

    /// 用于 `catalog_key` 的稳定标识。同一个 base_url 但 adapter 不同必须是不同目录
    /// （否则 Trae 与 WorkBuddy 若恰好共用域名会误共享模型列表与价格）。
    pub fn key_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Trae => "trae",
            Self::WorkBuddy => "workbuddy",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI Compatible",
            Self::Anthropic => "Anthropic",
            Self::Trae => "Trae",
            Self::WorkBuddy => "WorkBuddy",
        }
    }
}

impl Default for PlatformAdapter {
    fn default() -> Self {
        Self::OpenAI
    }
}

/// 上游额度的计量单位。决定这个平台的额度**怎么记账**。
///
/// 这个字段是必须的，不能靠 adapter 猜：同一个 `OpenAI` adapter 下，
/// Agnes 国内是 Credits（1000 = ¥1）、Agnes 海外是请求次数、魔搭是每日调用次数——
/// 猜不出来，只能让用户显式选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CreditUnit {
    /// 不计费。默认值，既有平台全走这个。
    None,
    /// 积分。走 `platform_credit_state` 台账，按自然窗口重置。
    /// 只适合窗口确实是自然日/周/月的上游（如 WorkBuddy）。
    Credits,
    /// 金额。走现有 `pricing` / `cost` 计价，不进积分台账。
    /// 适合本质是钱的上游（Trae 海外站的 Usage、Agnes 国内站的 Credits）。
    Currency,
    /// 请求次数。不建额度台账，只用 RPM 限流 + 上游余额同步。
    /// 适合窗口不是自然窗口的上游（Agnes 海外 5 小时窗口、Trae 31 天钱包、魔搭每日次数）。
    Requests,
}

impl CreditUnit {
    pub fn all() -> Vec<Self> {
        vec![Self::None, Self::Credits, Self::Currency, Self::Requests]
    }

    /// 大小写不敏感，理由同 `PlatformAdapter::resolve`：
    /// `key_str()` 写的是小写，而 serde 只认 PascalCase。
    pub fn resolve(stored: Option<&str>) -> Self {
        match stored.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => match raw.to_ascii_lowercase().as_str() {
                "credits" => Self::Credits,
                "currency" => Self::Currency,
                "requests" => Self::Requests,
                _ => Self::None,
            },
            None => Self::None,
        }
    }

    /// 是否走 `platform_credit_state` 台账（try_reserve / release）。
    pub fn uses_credit_ledger(&self) -> bool {
        matches!(self, Self::Credits)
    }

    pub fn key_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Credits => "credits",
            Self::Currency => "currency",
            Self::Requests => "requests",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Credits => "Credits",
            Self::Currency => "Currency",
            Self::Requests => "Requests",
        }
    }
}

impl Default for CreditUnit {
    fn default() -> Self {
        Self::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_defaults_from_platform_type() {
        assert_eq!(
            PlatformAdapter::resolve(None, &PlatformType::Anthropic),
            PlatformAdapter::Anthropic
        );
        assert_eq!(
            PlatformAdapter::resolve(None, &PlatformType::OpenAI),
            PlatformAdapter::OpenAI
        );
        // Ollama / Azure / Custom 都走 OpenAI 协议
        assert_eq!(
            PlatformAdapter::resolve(None, &PlatformType::Ollama),
            PlatformAdapter::OpenAI
        );
    }

    #[test]
    fn adapter_parses_stored_value() {
        assert_eq!(
            PlatformAdapter::resolve(Some("Trae"), &PlatformType::OpenAI),
            PlatformAdapter::Trae
        );
        assert_eq!(
            PlatformAdapter::resolve(Some("WorkBuddy"), &PlatformType::OpenAI),
            PlatformAdapter::WorkBuddy
        );
        // 脏数据回落到由 type 推出的默认值，而不是 panic
        assert_eq!(
            PlatformAdapter::resolve(Some("garbage"), &PlatformType::Anthropic),
            PlatformAdapter::Anthropic
        );
    }

    #[test]
    fn adapter_key_roundtrips_through_resolve() {
        // 关键回归：key_str() 写入小写，resolve() 必须能读回同一个值。
        // 若两者不一致，「存进去再读出来」会静默退化成默认 adapter，
        // 私有协议上游会被当成 OpenAI 兼容上游发请求。
        for adapter in PlatformAdapter::all() {
            assert_eq!(
                PlatformAdapter::resolve(Some(adapter.key_str()), &PlatformType::OpenAI),
                adapter,
                "{} 的 key_str 无法被 resolve 读回",
                adapter.key_str()
            );
        }
    }

    #[test]
    fn credit_unit_key_roundtrips_through_resolve() {
        for unit in CreditUnit::all() {
            assert_eq!(
                CreditUnit::resolve(Some(unit.key_str())),
                unit,
                "{} 的 key_str 无法被 resolve 读回",
                unit.key_str()
            );
        }
    }

    #[test]
    fn proprietary_adapters_are_flagged() {
        assert!(PlatformAdapter::Trae.is_proprietary());
        assert!(PlatformAdapter::WorkBuddy.is_proprietary());
        assert!(!PlatformAdapter::OpenAI.is_proprietary());
        assert!(!PlatformAdapter::Anthropic.is_proprietary());
    }

    #[test]
    fn adapter_keys_are_distinct() {
        let keys: Vec<&str> = PlatformAdapter::all().iter().map(|a| a.key_str()).collect();
        let mut deduped = keys.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(keys.len(), deduped.len(), "adapter key 必须互不相同");
    }

    #[test]
    fn only_credits_uses_ledger() {
        assert!(CreditUnit::Credits.uses_credit_ledger());
        assert!(!CreditUnit::Currency.uses_credit_ledger());
        assert!(!CreditUnit::Requests.uses_credit_ledger());
        assert!(!CreditUnit::None.uses_credit_ledger());
    }

    #[test]
    fn credit_unit_resolves_none_for_missing_or_garbage() {
        assert_eq!(CreditUnit::resolve(None), CreditUnit::None);
        assert_eq!(CreditUnit::resolve(Some("nonsense")), CreditUnit::None);
        assert_eq!(CreditUnit::resolve(Some("Credits")), CreditUnit::Credits);
        // 小写是 key_str 的写法，必须能读回
        assert_eq!(CreditUnit::resolve(Some("currency")), CreditUnit::Currency);
        assert_eq!(CreditUnit::resolve(Some("requests")), CreditUnit::Requests);
    }
}
