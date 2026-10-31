//! 魔搭 ModelScope 的每日魔粒适配器。
//!
//! 事实（2026-10-31 抓取线上前端确认）：
//! - 魔粒接口基址 `https://www.modelscope.cn/openapi`，Cookie 认证（会话 cookie `m_session_id`），
//!   GET 不需要 CSRF token；
//! - `GET /v1/magicubes/balance` 返回 `{success, data:{total_balance, available_balance}}`；
//! - `GET /v1/magicubes/earn/rules` 的 `data` 是规则数组，`category_key=daily` 是每日基础奖励；
//! - 页面没有「签到按钮」：规则完成即到账，`today_remain == 0` 表示今天已领；
//! - 未登录返回 HTTP 401 + `code=InvalidAuthentication`。
//!
//! 因此「签到」= 带着登录态访问一次这两个接口，再做一次余额与规则核对。

use std::time::Duration;

use serde_json::Value;

/// 登录后拿到的账号信息：`name` 用来区分同平台的多个账号，
/// `api_key` 是从「访问令牌」列表里挑出的 SDK 令牌（没有就是 None）。
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub name: String,
    pub api_key: Option<String>,
}

impl AccountInfo {
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            "ModelScope 魔搭".to_string()
        } else {
            format!("ModelScope 魔搭 · {}", self.name)
        }
    }
}

pub const BASE_URL: &str = "https://www.modelscope.cn";
pub const LOGIN_URL: &str = "https://www.modelscope.cn/magicube/usage?tab=earn";
pub const SESSION_COOKIE: &str = "m_session_id";

const REFERER: &str = "https://www.modelscope.cn/magicube/usage?tab=earn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct Pulse {
    pub balance: f64,
    pub available: f64,
    pub daily_done: bool,
    pub daily_amount: Option<f64>,
    pub daily_total: usize,
    pub daily_completed: usize,
}

#[derive(Debug)]
pub enum PulseError {
    /// 登录态失效（HTTP 401 / InvalidAuthentication）
    Expired,
    Failed(String),
}

fn build_client() -> Result<reqwest::Client, PulseError> {
    crate::config::SystemProxy::detect()
        .apply_to_builder(reqwest::Client::builder())
        .build()
        .map_err(|e| PulseError::Failed(format!("创建 HTTP 客户端失败: {e}")))
}

pub async fn pulse(cookie: &str) -> Result<Pulse, PulseError> {
    pulse_with(&build_client()?, BASE_URL, cookie).await
}

/// 登录完成后用 cookie 探测账号：拿到账号名（多账号区分用）和 SDK 令牌。
/// 用户信息接口的旧信封是 `{Success, Data:{Name}}`，未登录返回 HTTP 401 / Code 10010101003。
pub async fn probe_account(cookie: &str) -> Result<AccountInfo, PulseError> {
    probe_account_with(&build_client()?, BASE_URL, cookie).await
}

pub async fn probe_account_with(
    client: &reqwest::Client,
    base: &str,
    cookie: &str,
) -> Result<AccountInfo, PulseError> {
    let info = get_legacy_json(client, &format!("{base}/api/v1/users/info"), cookie).await?;
    let name = info
        .get("Data")
        .and_then(|data| data.get("Name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // 令牌列表失败不影响登录态：有的账号一个 SDK 令牌都没有，仍可以先领取魔粒
    let api_key = match get_legacy_json(
        client,
        &format!("{base}/api/v1/users/tokens/list"),
        cookie,
    )
    .await
    {
        Ok(body) => pick_token(&body),
        Err(PulseError::Expired) => return Err(PulseError::Expired),
        Err(_) => None,
    };

    Ok(AccountInfo { name, api_key })
}

/// 从 `/api/v1/users/tokens/list` 的响应里挑第一个未过期的 SDK 令牌。
/// 字段名没有文档，先按常见键取，再兜底扫描 `ms-` 前缀的字符串。
pub fn pick_token(body: &Value) -> Option<String> {
    let tokens = body
        .get("Data")
        .and_then(|data| data.get("SdkTokens"))
        .and_then(Value::as_array)?;
    for token in tokens {
        if token_expired(token) {
            continue;
        }
        for key in ["Value", "Token", "TokenValue", "SdkToken", "AccessKey", "Key"] {
            if let Some(value) = token.get(key).and_then(Value::as_str) {
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        if let Some(object) = token.as_object() {
            for value in object.values() {
                if let Some(text) = value.as_str() {
                    if text.starts_with("ms-") && text.len() > 10 {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

fn token_expired(token: &Value) -> bool {
    for key in ["Expired", "IsExpired"] {
        if token.get(key).and_then(Value::as_bool) == Some(true) {
            return true;
        }
    }
    for key in ["ExpireTime", "ExpirationTime", "ExpiresAt", "ExpireAt"] {
        let Some(value) = token.get(key) else { continue };
        let timestamp = match value {
            Value::Number(number) => number.as_i64().map(|raw| {
                // 秒与毫秒两种都有可能
                if raw > 10_000_000_000 { raw / 1000 } else { raw }
            }),
            Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|at| at.timestamp()),
            _ => None,
        };
        if let Some(at) = timestamp {
            return at <= chrono::Utc::now().timestamp();
        }
    }
    false
}

/// 平台 Web 接口（`/api/v1/*`）用的是旧信封：`{Success, Data, Message, Code}`。
async fn get_legacy_json(
    client: &reqwest::Client,
    url: &str,
    cookie: &str,
) -> Result<Value, PulseError> {
    let response = client
        .get(url)
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, UA)
        .header(reqwest::header::REFERER, REFERER)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| PulseError::Failed(format!("请求失败: {e}")))?;

    if matches!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(PulseError::Expired);
    }
    if !response.status().is_success() {
        return Err(PulseError::Failed(format!("上游返回 HTTP {}", response.status())));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|e| PulseError::Failed(format!("响应不是 JSON: {e}")))?;
    if body.get("Success").and_then(Value::as_bool) != Some(true) {
        // 10010101003 = user not logged in
        if body.get("Code").and_then(Value::as_i64) == Some(10010101003) {
            return Err(PulseError::Expired);
        }
        let message = body
            .get("Message")
            .and_then(Value::as_str)
            .unwrap_or("上游返回 Success=false");
        return Err(PulseError::Failed(message.to_string()));
    }
    Ok(body)
}

pub async fn pulse_with(
    client: &reqwest::Client,
    base: &str,
    cookie: &str,
) -> Result<Pulse, PulseError> {
    // 先按「打开网页」访问一次：每日活跃类规则更可能挂在页面访问上。
    // 失败不影响后续核对——登录态是否有效由 balance 的 401 判定。
    let _ = client
        .get(format!("{base}/magicube/usage?tab=earn"))
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::USER_AGENT, UA)
        .timeout(Duration::from_secs(20))
        .send()
        .await;

    let balance_url = format!("{base}/openapi/v1/magicubes/balance");
    let rules_url = format!("{base}/openapi/v1/magicubes/earn/rules");

    let balance_body = get_json(client, &balance_url, cookie).await?;
    let (balance, available) = parse_balance(&balance_body)?;

    let rules_body = get_json(client, &rules_url, cookie).await?;
    let (daily_done, daily_amount, daily_total, daily_completed) = daily_from_rules(&rules_body);

    Ok(Pulse { balance, available, daily_done, daily_amount, daily_total, daily_completed })
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    cookie: &str,
) -> Result<Value, PulseError> {
    let response = client
        .get(url)
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, UA)
        .header(reqwest::header::REFERER, REFERER)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| PulseError::Failed(format!("请求失败: {e}")))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(PulseError::Expired);
    }
    if !response.status().is_success() {
        return Err(PulseError::Failed(format!("上游返回 HTTP {}", response.status())));
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| PulseError::Failed(format!("响应不是 JSON: {e}")))?;
    if body.get("success").and_then(Value::as_bool) != Some(true) {
        let code = body.get("code").and_then(Value::as_str).unwrap_or("");
        if code == "InvalidAuthentication" {
            return Err(PulseError::Expired);
        }
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("上游返回 success=false");
        return Err(PulseError::Failed(message.to_string()));
    }
    Ok(body)
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn field(data: &Value, key: &str) -> Option<f64> {
    data.get(key).and_then(number)
}

pub fn parse_balance(body: &Value) -> Result<(f64, f64), PulseError> {
    let data = body
        .get("data")
        .ok_or_else(|| PulseError::Failed("余额响应缺少 data".to_string()))?;
    let total = field(data, "total_balance")
        .ok_or_else(|| PulseError::Failed("余额响应缺少 total_balance".to_string()))?;
    let available = field(data, "available_balance").unwrap_or(total);
    Ok((total, available))
}

/// 解析每日基础奖励：`category_key == "daily"` 的规则里，已完成的条数与金额之和。
/// 完成条件来自前端实现：短期限看 `today_remain == 0`，长期限看总额已领完。
pub fn daily_from_rules(body: &Value) -> (bool, Option<f64>, usize, usize) {
    let Some(rules) = body.get("data").and_then(Value::as_array) else {
        return (false, None, 0, 0);
    };
    let mut total = 0usize;
    let mut completed = 0usize;
    let mut amount = 0.0f64;
    for rule in rules {
        if rule.get("category_key").and_then(Value::as_str) != Some("daily") {
            continue;
        }
        total += 1;
        let scene = rule.get("scene").and_then(Value::as_str).unwrap_or("");
        let done = match scene {
            "short_term" => field(rule, "today_remain") == Some(0.0),
            "long_term" => {
                let cap = field(rule, "total_cap").unwrap_or(0.0);
                cap != 0.0 && field(rule, "total_earned") == Some(cap)
            }
            _ => false,
        };
        if done {
            completed += 1;
            amount += field(rule, "amount").unwrap_or(0.0);
        }
    }
    let daily_amount = if completed > 0 { Some(amount) } else { None };
    (completed > 0, daily_amount, total, completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn balance_accepts_int_and_string() {
        let body = json!({"success": true, "data": {"total_balance": 1234, "available_balance": "1000.5"}});
        assert_eq!(parse_balance(&body).unwrap(), (1234.0, 1000.5));

        // 老接口只有总额时，可用额度按总额算，不报错
        let body = json!({"success": true, "data": {"total_balance": 20}});
        assert_eq!(parse_balance(&body).unwrap(), (20.0, 20.0));

        let body = json!({"success": true, "data": {}});
        assert!(matches!(parse_balance(&body), Err(PulseError::Failed(_))));
    }

    #[test]
    fn daily_rules_count_only_completed_daily_category() {
        let body = json!({
            "success": true,
            "data": [
                {"category_key": "daily", "rule_key": "daily_login", "title": "每日发放",
                 "scene": "short_term", "amount": 100, "daily_cap": 1, "today_remain": 0, "today_used": 1},
                {"category_key": "daily", "rule_key": "bind_aliyun", "title": "绑阿里云账号",
                 "scene": "long_term", "amount": 50, "total_cap": 1, "total_earned": 0},
                {"category_key": "interaction", "rule_key": "like", "title": "赞评互动",
                 "scene": "short_term", "amount": 10, "today_remain": 0},
                {"category_key": "daily", "rule_key": "read_paper", "title": "每日读论文",
                 "scene": "short_term", "amount": 5, "today_remain": 2}
            ]
        });
        let (done, amount, total, completed) = daily_from_rules(&body);
        assert!(done);
        assert_eq!(amount, Some(100.0));
        assert_eq!((total, completed), (3, 1));
    }

    #[test]
    fn daily_rules_handle_not_yet_claimed_and_empty() {
        let body = json!({"success": true, "data": [
            {"category_key": "daily", "scene": "short_term", "amount": 100, "today_remain": 1}
        ]});
        let (done, amount, total, completed) = daily_from_rules(&body);
        assert!(!done);
        assert_eq!(amount, None);
        assert_eq!((total, completed), (1, 0));

        let empty = json!({"success": true, "data": []});
        assert_eq!(daily_from_rules(&empty), (false, None, 0, 0));

        let broken = json!({"success": true});
        assert_eq!(daily_from_rules(&broken), (false, None, 0, 0));
    }

    #[test]
    fn pick_token_skips_expired_and_prefers_named_field() {
        let body = json!({"Success": true, "Data": {"SdkTokens": [
            {"Id": 1, "Name": "old", "Value": "ms-old-token-0001", "Expired": true},
            {"Id": 2, "Name": "gw", "ExpireTime": 4102444800i64, "TokenValue": "ms-live-token-0002"},
            {"Id": 3, "Name": "fallback", "Anything": "ms-second-token-0003"}
        ]}});
        assert_eq!(pick_token(&body).as_deref(), Some("ms-live-token-0002"));

        // 只有兜底字段时按 ms- 前缀认
        let body = json!({"Success": true, "Data": {"SdkTokens": [
            {"Id": 1, "Name": "fallback", "Anything": "ms-second-token-0003"}
        ]}});
        assert_eq!(pick_token(&body).as_deref(), Some("ms-second-token-0003"));

        assert_eq!(pick_token(&json!({"Success": true, "Data": {"SdkTokens": []}})), None);
        assert_eq!(pick_token(&json!({"Success": true})), None);
    }

    #[test]
    fn token_expiry_understands_seconds_and_iso() {
        assert!(token_expired(&json!({"ExpireTime": 1_600_000_000i64})));
        assert!(!token_expired(&json!({"ExpireTime": "2999-01-01T00:00:00Z"})));
        // 没有过期字段 = 不知道，不等于已过期
        assert!(!token_expired(&json!({"Name": "x"})));
    }

    #[test]
    fn long_term_rule_completes_when_cap_reached() {
        let body = json!({"success": true, "data": [
            {"category_key": "daily", "scene": "long_term", "amount": 50, "total_cap": 2, "total_earned": 2}
        ]});
        assert_eq!(daily_from_rules(&body), (true, Some(50.0), 1, 1));
    }
}
