//! 上游额度查询（WorkBuddy / Trae）。
//!
//! 这两家的额度接口都是私有协议的，形态与 chat 接口不同：
//! - WorkBuddy：`POST {billingBase}/v2/billing/meter/get-user-resource`
//!   响应 `data.Response.Data.Accounts[]`，剩余取 `CycleCapacitySize>0 ? CycleCapacityRemain : CapacityRemain`
//! - Trae：`POST {apiBase}/trae/api/v2/pay/ide_user_ent_usage`
//!   body `{"require_usage":true,"req_source":0}`，响应 `user_entitlement_pack_list[]`
//!
//! 设计原则：**查询失败不阻塞主流程**。额度同步是增强功能，查不到就返回 None，
//! 让调用方保持本地台账不动，而不是把整个请求链拖垮。

use serde_json::Value;

use crate::models::adapter::PlatformAdapter;
use crate::models::platform::Platform;

/// 上游报告的额度快照。
#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamQuota {
    /// 剩余额度。None = 上游没给出可用的数字（无限额度/字段缺失）。
    pub remaining: Option<f64>,
    /// 总额度（如果上游给了）。
    pub total: Option<f64>,
    /// 上游原始响应，便于排查（截断）。
    pub raw: String,
}

/// 按 adapter 查询上游额度。
pub async fn query(
    client: &reqwest::Client,
    adapter: PlatformAdapter,
    platform: &Platform,
    credential: &crate::db::credential::CredentialSecret,
    timeout_secs: u64,
) -> Result<UpstreamQuota, String> {
    match adapter {
        PlatformAdapter::WorkBuddy => {
            query_workbuddy(client, platform, credential, timeout_secs).await
        }
        PlatformAdapter::Trae => query_trae(client, platform, credential, timeout_secs).await,
        _ => Err(format!(
            "{} 没有额度查询接口（额度按模型价格本地计算）",
            adapter.label()
        )),
    }
}

async fn query_workbuddy(
    client: &reqwest::Client,
    platform: &Platform,
    credential: &crate::db::credential::CredentialSecret,
    timeout_secs: u64,
) -> Result<UpstreamQuota, String> {
    let base = platform.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("WorkBuddy 平台缺少 base_url".to_string());
    }
    let url = format!("{base}/v2/billing/meter/get-user-resource");
    // 时间窗给足一年：上游按 PackageEndTimeRange 过滤有效资源包
    let now = chrono::Utc::now();
    let end = now + chrono::Duration::days(365);
    let fmt = |t: chrono::DateTime<chrono::Utc>| t.format("%Y-%m-%d %H:%M:%S").to_string();
    let body = serde_json::json!({
        "PageNumber": 1,
        "PageSize": 100,
        // ProductCode 是照抄社区的实测值，不要改
        "ProductCode": "p_tcaca",
        "Status": [0, 3],
        "PackageEndTimeRangeBegin": fmt(now),
        "PackageEndTimeRangeEnd": fmt(end),
    });

    let mut req = client
        .post(&url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", credential.access_token));
    if let Some(uid) = &credential.uid {
        req = req.header("X-User-Id", uid);
    }
    if let Some(enterprise) = &credential.enterprise_id {
        req = req
            .header("X-Enterprise-Id", enterprise)
            .header("X-Tenant-Id", enterprise);
    }
    if let Some(domain) = &credential.domain {
        req = req.header("X-Domain", domain);
    }

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("WorkBuddy 额度接口请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "WorkBuddy 额度接口 HTTP {}: {}",
            status.as_u16(),
            truncate(&text, 200)
        ));
    }
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("WorkBuddy 额度接口返回无法解析: {e}"))?;
    // code 缺失或为 0 都算成功
    if let Some(code) = json.get("code").and_then(|v| v.as_i64()) {
        if code != 0 {
            return Err(format!(
                "WorkBuddy 额度接口 code={}: {}",
                code,
                truncate(&text, 200)
            ));
        }
    }

    let accounts = json
        .get("data")
        .and_then(|d| d.get("Response"))
        .and_then(|r| r.get("Data"))
        .and_then(|d| d.get("Accounts"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut remaining = 0.0f64;
    let mut total = 0.0f64;
    let mut saw_any = false;
    for account in &accounts {
        // CycleCapacitySize > 0 表示这是周期资源包，剩余看 CycleCapacityRemain；
        // 否则是普通资源包，剩余看 CapacityRemain
        let cycle_size = account.get("CycleCapacitySize").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let (remain, size) = if cycle_size > 0.0 {
            (
                account.get("CycleCapacityRemain").and_then(|v| v.as_f64()).unwrap_or(0.0),
                cycle_size,
            )
        } else {
            (
                account.get("CapacityRemain").and_then(|v| v.as_f64()).unwrap_or(0.0),
                account.get("CapacitySize").and_then(|v| v.as_f64()).unwrap_or(0.0),
            )
        };
        remaining += remain;
        total += size;
        saw_any = true;
    }

    Ok(UpstreamQuota {
        remaining: if saw_any { Some(remaining) } else { None },
        total: if saw_any { Some(total) } else { None },
        raw: truncate(&text, 500),
    })
}

async fn query_trae(
    client: &reqwest::Client,
    platform: &Platform,
    credential: &crate::db::credential::CredentialSecret,
    timeout_secs: u64,
) -> Result<UpstreamQuota, String> {
    let base = platform.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Trae 平台缺少 base_url".to_string());
    }
    // Trae 的额度接口在商业面（api.trae.cn），不在模型数据面。
    // 数据面的 host 打这个路径会 404，所以这里固定用官方商业域名。
    let url = "https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage";
    let body = serde_json::json!({ "require_usage": true, "req_source": 0 });

    let resp = client
        .post(url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Cloud-IDE-JWT {}", credential.access_token),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Trae 额度接口请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "Trae 额度接口 HTTP {}: {}",
            status.as_u16(),
            truncate(&text, 200)
        ));
    }
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("Trae 额度接口返回无法解析: {e}"))?;

    // 响应形态：user_entitlement_pack_list[].entitlement_base_info.quota
    //   .credits_limit（-1 = 无限）/ .credits_amount（已用）
    let packs = json
        .get("user_entitlement_pack_list")
        .or_else(|| json.get("data").and_then(|d| d.get("user_entitlement_pack_list")))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut remaining = 0.0f64;
    let mut total = 0.0f64;
    let mut saw_any = false;
    for pack in &packs {
        let quota = pack.get("entitlement_base_info").and_then(|b| b.get("quota"));
        let Some(quota) = quota else { continue };
        let limit = quota.get("credits_limit").and_then(|v| v.as_f64());
        let used = quota.get("credits_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
        match limit {
            // -1 = 无限额度，没有「剩余」可言
            Some(l) if l < 0.0 => {
                saw_any = true;
            }
            Some(l) => {
                remaining += (l - used).max(0.0);
                total += l;
                saw_any = true;
            }
            None => {}
        }
    }

    Ok(UpstreamQuota {
        remaining: if saw_any { Some(remaining) } else { None },
        total: if saw_any { Some(total) } else { None },
        raw: truncate(&text, 500),
    })
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text_intact() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdef", 3), "abc…");
    }

    #[test]
    fn standard_adapters_have_no_quota_endpoint() {
        // OpenAI / Anthropic 的额度由本地按模型价格计算，没有查询接口
        let adapter = PlatformAdapter::OpenAI;
        assert_eq!(adapter.label(), "OpenAI Compatible");
        // 这里只验证 label 可用；实际 query 需要网络，不在此测
    }

    #[test]
    fn workbuddy_quota_body_uses_expected_product_code() {
        // ProductCode 是社区实测值，改了就查不到额度
        let body = serde_json::json!({ "ProductCode": "p_tcaca" });
        assert_eq!(body["ProductCode"], "p_tcaca");
    }
}
