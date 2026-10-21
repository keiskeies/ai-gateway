use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::models::model::NullableNumber;

fn default_enabled() -> bool {
    true
}

fn keep_number() -> NullableNumber { NullableNumber::Keep }

/// 可空文本列的三种意图：JSON 里没写＝保持原值，写 `null`＝清空，写字符串＝设值。
/// 与 `NullableNumber` 同一套理由：`Option<Option<String>>` 里"没写"和"写了 null"会塌成同一个值，
/// 于是"把 Key 改回全局"（清空 proxy_id）这种操作根本表达不出来。
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NullableString {
    #[default]
    Keep,
    Clear,
    Set(String),
}

struct NullableStringVisitor;

impl<'de> serde::de::Visitor<'de> for NullableStringVisitor {
    type Value = NullableString;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a string or null")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(NullableString::Clear)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(NullableString::Clear)
    }

    fn visit_some<D: serde::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(NullableStringVisitor)
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(NullableString::Set(v.to_string()))
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(NullableString::Set(v))
    }
}

impl<'de> Deserialize<'de> for NullableString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NullableStringVisitor)
    }
}

fn keep_string() -> NullableString { NullableString::Keep }

/// 配额窗口。NULL＝累计（V18 的老口径），其余是滚动窗口：分母只统计最近这一段
/// `request_stats`，所以"每月 2 元"不需要谁去把已用量清零。
pub const QUOTA_WINDOWS: [&str; 3] = ["day", "week", "month"];

pub fn quota_window_seconds(window: &str) -> Option<i64> {
    match window {
        "day" => Some(86_400),
        "week" => Some(604_800),
        "month" => Some(2_592_000),
        _ => None,
    }
}

pub enum WindowInput {
    /// 累计：库里存 NULL
    Lifetime,
    Window(String),
    /// 认不出来的写法——接口按 400 处理，不能悄悄当成"不限"
    Invalid,
}

pub fn parse_quota_window(raw: &str) -> WindowInput {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "lifetime" || normalized == "all" {
        return WindowInput::Lifetime;
    }
    if QUOTA_WINDOWS.contains(&normalized.as_str()) {
        return WindowInput::Window(normalized);
    }
    WindowInput::Invalid
}

/// 落库前的归一：库里只允许出现 `day`/`week`/`month` 三个字面量或 NULL。
/// 接口层已经按 400 挡掉非法值，这里再归一次是因为走偏的后果是静默的——
/// `usage_by_key` 按 `quota_window` 挑时间下界，闸门按 `window_start` 挑，
/// 存进一个认不出的字符串会让两处走不同分支，界面上的分子和真正拒人的分母就此错开。
pub fn canonical_quota_window(raw: Option<&str>) -> Option<&'static str> {
    let parsed = raw.map(parse_quota_window)?;
    match parsed {
        WindowInput::Window(name) => QUOTA_WINDOWS.into_iter().find(|w| *w == name),
        _ => None,
    }
}

/// 窗口起点（SQL 里和 `created_at` 直接比字符串，与告警/统计其它时间窗同一口径）。
/// 累计口径没有起点，返回 None。
pub fn window_start(window: Option<&str>, now: DateTime<Utc>) -> Option<String> {
    let seconds = window.and_then(quota_window_seconds)?;
    let from = now - chrono::Duration::seconds(seconds);
    Some(from.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
}

/// 一把 Key 的已用量，同时是配额的分母和界面上的分子——两处必须同源。
/// `cost` 只累加已定价的请求（未定价行 `cost` 是 NULL，不能当 0 计），
/// 所以它总是和 `priced_requests` / `requests` 一起出现，覆盖率不说出来就是撒谎。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KeyUsage {
    pub requests: i64,
    pub tokens: i64,
    pub cost: Option<f64>,
    pub priced_requests: i64,
}

/// 解析到期时间。前端给的是带时区的 RFC3339；也接受不带时区的 `YYYY-MM-DDTHH:MM[:SS]`
/// （按 UTC 理解）与裸日期 `YYYY-MM-DD`（当天 23:59:59 UTC，即"这天结束前还能用"）。
/// 解析不了就当作没设——宁可放行也不要用一个猜出来的时刻拒掉真实请求。
pub fn parse_expiry(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let end_of_day = date.and_hms_opt(23, 59, 59)?;
        return Some(Utc.from_utc_datetime(&end_of_day));
    }
    None
}

/// 管理接口与前端看到的 Key：`key` 一律是掩码，永远不可能是可用凭据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    /// 掩码（`sk-a••••••••9f3c` 这种形态）。库内只存哈希，任何接口都取不出完整明文。
    pub key: String,
    pub proxy_id: Option<String>,
    pub created_at: String,
    pub last_used: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 到期时间（RFC3339）。NULL＝不过期。
    pub expires_at: Option<String>,
    /// 服务端算好的"已过期"：让前端不必自己解析各种时间格式再比时钟。
    pub expired: bool,
    pub quota_requests: Option<i64>,
    pub quota_tokens: Option<i64>,
    /// 预算配额（按成本，单位与成本读数一致：每 100 万 token 的单价算出来的金额）。
    /// NULL＝不限。只统计已定价的请求，覆盖率随 `priced_requests` 一起给出。
    pub quota_cost: Option<f64>,
    /// 配额窗口：NULL＝累计，`day`/`week`/`month`＝滚动窗口。三个配额共用同一个窗口。
    pub quota_window: Option<String>,
    /// 配额的分母对应分子：窗口内现存 `request_stats` 的用量。
    /// 统计行会被删（删除虚拟模型、清理统计），所以这是"现存统计里的用量"，不是不可变台账。
    pub used_requests: i64,
    pub used_tokens: i64,
    pub used_cost: Option<f64>,
    /// 窗口内已定价的请求数：`used_cost` 的覆盖率分母是 `used_requests`。
    pub priced_requests: i64,
    /// 一次性明文：只出现在"创建"和"轮换"那一次响应里，其余接口不带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// 落库形状：明文不落盘，只存 sha256 摘要与展示用掩码。
#[derive(Debug, Clone)]
pub struct StoredApiKey {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub key_mask: String,
    pub proxy_id: Option<String>,
    pub created_at: String,
    pub last_used: Option<String>,
    pub enabled: bool,
    pub expires_at: Option<String>,
    pub quota_requests: Option<i64>,
    pub quota_tokens: Option<i64>,
    pub quota_cost: Option<f64>,
    pub quota_window: Option<String>,
}

impl StoredApiKey {
    /// 到没到期。已存的时间戳解析不了时按"未到期"处理——与写入口径一致，
    /// 不会因为一个手写坏的字符串就凭空拒掉请求。
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .as_deref()
            .and_then(parse_expiry)
            .map(|until| until <= now)
            .unwrap_or(false)
    }

    /// 有没有设任何配额——没设的 Key 不必为判定多跑一次统计查询。
    pub fn has_quota(&self) -> bool {
        self.quota_requests.is_some() || self.quota_tokens.is_some() || self.quota_cost.is_some()
    }

    /// 转成对外形状（掩码），明文无从生成——到这里已经没有明文了。
    /// `used` 由调用方从统计里查好后传入：这里拿不到数据库。
    pub fn to_public(&self, used: KeyUsage) -> ApiKey {
        ApiKey {
            id: self.id.clone(),
            name: self.name.clone(),
            key: self.key_mask.clone(),
            proxy_id: self.proxy_id.clone(),
            created_at: self.created_at.clone(),
            last_used: self.last_used.clone(),
            enabled: self.enabled,
            expires_at: self.expires_at.clone(),
            expired: self.is_expired(Utc::now()),
            quota_requests: self.quota_requests,
            quota_tokens: self.quota_tokens,
            quota_cost: self.quota_cost,
            quota_window: self.quota_window.clone(),
            used_requests: used.requests,
            used_tokens: used.tokens,
            used_cost: used.cost,
            priced_requests: used.priced_requests,
            secret: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// 可选：客户端固定 key；为空时由服务端生成。明文只用于当场取哈希，不落库。
    #[serde(default)]
    pub key: Option<String>,
    pub proxy_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub quota_requests: Option<i64>,
    #[serde(default)]
    pub quota_tokens: Option<i64>,
    #[serde(default)]
    pub quota_cost: Option<f64>,
    /// `lifetime`/`all`/空＝累计，`day`/`week`/`month`＝滚动窗口；认不出来按 400 处理。
    #[serde(default)]
    pub quota_window: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    /// 改绑：`null` 变全局，字符串换到另一个虚拟模型，不写就不动。
    #[serde(default = "keep_string")]
    pub proxy_id: NullableString,
    /// 到期时间同样三态：`null` 表示"改成不过期"。
    #[serde(default = "keep_string")]
    pub expires_at: NullableString,
    #[serde(default = "keep_number")]
    pub quota_requests: NullableNumber,
    #[serde(default = "keep_number")]
    pub quota_tokens: NullableNumber,
    #[serde(default = "keep_number")]
    pub quota_cost: NullableNumber,
    /// 窗口也是三态：`null` 回到累计口径。
    #[serde(default = "keep_string")]
    pub quota_window: NullableString,
}

/// 批量动作只有三种，没有"顺便改名称/配额"——那些字段每把 Key 都不同，
/// 批量写它们没有共同语义，只会误伤。轮换也不在这里：新值只能一次性回显。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchApiKeyRequest {
    pub ids: Vec<String>,
    pub action: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_parsing_covers_what_the_ui_sends() {
        // 前端 datetime-local → new Date(v).toISOString()
        assert!(parse_expiry("2030-01-02T03:04:05.000Z").is_some());
        assert!(parse_expiry("2030-01-02T03:04:05+08:00").is_some());
        assert_eq!(
            parse_expiry("2030-01-02T03:04"),
            parse_expiry("2030-01-02T03:04:00")
        );
        // 裸日期是"当天结束前"，不是当天零点：否则选了今天当天就立刻过期。
        assert_eq!(parse_expiry("2030-01-02"), parse_expiry("2030-01-02T23:59:59"));
        assert_eq!(parse_expiry(""), None);
        assert_eq!(parse_expiry("tomorrow"), None);
        let beijing = parse_expiry("2030-01-02T03:04:00+08:00").unwrap();
        assert_eq!(beijing, parse_expiry("2030-01-01T19:04:00Z").unwrap());
    }

    #[test]
    fn update_request_tells_absent_from_null() {
        let absent: UpdateApiKeyRequest = serde_json::from_str(r#"{"name":"n"}"#).unwrap();
        assert_eq!(absent.proxy_id, NullableString::Keep);
        assert_eq!(absent.expires_at, NullableString::Keep);
        assert_eq!(absent.quota_requests, NullableNumber::Keep);

        let cleared: UpdateApiKeyRequest =
            serde_json::from_str(r#"{"proxy_id":null,"expires_at":null,"quota_requests":null}"#).unwrap();
        assert_eq!(cleared.proxy_id, NullableString::Clear);
        assert_eq!(cleared.expires_at, NullableString::Clear);
        assert_eq!(cleared.quota_requests, NullableNumber::Clear);

        let set: UpdateApiKeyRequest =
            serde_json::from_str(r#"{"proxy_id":"p1","quota_tokens":5000}"#).unwrap();
        assert_eq!(set.proxy_id, NullableString::Set("p1".to_string()));
        assert_eq!(set.quota_tokens, NullableNumber::Set(5000.0));

        // 配额给字符串：报类型错误而不是悄悄当成没写
        let bad = serde_json::from_str::<UpdateApiKeyRequest>(r#"{"quota_requests":"many"}"#);
        assert!(bad.is_err());
    }

    #[test]
    fn expired_flag_follows_the_stored_timestamp() {
        let now = Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap();
        let mut key = StoredApiKey {
            id: "k".into(),
            name: "k".into(),
            key_hash: "h".into(),
            key_mask: "m".into(),
            proxy_id: None,
            created_at: now.to_rfc3339(),
            last_used: None,
            enabled: true,
            expires_at: None,
            quota_requests: None,
            quota_tokens: None,
            quota_cost: None,
            quota_window: None,
        };
        assert!(!key.is_expired(now));
        key.expires_at = Some("2026-10-20T11:59:59Z".into());
        assert!(key.is_expired(now));
        key.expires_at = Some("2026-10-20T12:00:01Z".into());
        assert!(!key.is_expired(now));
        // 手写坏的时间戳：不设限制，而不是永久拒绝
        key.expires_at = Some("not a timestamp".into());
        assert!(!key.is_expired(now));

        let public = key.to_public(KeyUsage {
            requests: 7,
            tokens: 4321,
            cost: Some(0.25),
            priced_requests: 4,
        });
        assert_eq!(public.key, "m");
        assert!(public.secret.is_none());
        assert_eq!(public.used_requests, 7);
        assert_eq!(public.used_tokens, 4321);
        assert_eq!(public.used_cost, Some(0.25));
        assert_eq!(public.priced_requests, 4);
    }

    #[test]
    fn quota_window_accepts_only_the_three_rolling_windows() {
        assert!(matches!(parse_quota_window("day"), WindowInput::Window(w) if w == "day"));
        // 大小写和空格不敏感：Select 之外还有人手工调接口
        assert!(matches!(parse_quota_window(" Week "), WindowInput::Window(w) if w == "week"));
        assert!(matches!(parse_quota_window(""), WindowInput::Lifetime));
        assert!(matches!(parse_quota_window("lifetime"), WindowInput::Lifetime));
        assert!(matches!(parse_quota_window("quarter"), WindowInput::Invalid));

        let now = Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap();
        assert_eq!(window_start(None, now), None);
        assert_eq!(window_start(Some("day"), now), Some("2026-10-19T12:00:00+00:00".into()));
        assert_eq!(window_start(Some("month"), now), Some("2026-09-20T12:00:00+00:00".into()));
    }

    #[test]
    fn cost_quota_and_window_are_tri_state_too() {
        let absent: UpdateApiKeyRequest = serde_json::from_str(r#"{"name":"n"}"#).unwrap();
        assert_eq!(absent.quota_cost, NullableNumber::Keep);
        assert_eq!(absent.quota_window, NullableString::Keep);

        let cleared: UpdateApiKeyRequest =
            serde_json::from_str(r#"{"quota_cost":null,"quota_window":null}"#).unwrap();
        assert_eq!(cleared.quota_cost, NullableNumber::Clear);
        assert_eq!(cleared.quota_window, NullableString::Clear);

        let set: UpdateApiKeyRequest =
            serde_json::from_str(r#"{"quota_cost":2.5,"quota_window":"month"}"#).unwrap();
        assert_eq!(set.quota_cost, NullableNumber::Set(2.5));
        assert_eq!(set.quota_window, NullableString::Set("month".to_string()));
    }
}
