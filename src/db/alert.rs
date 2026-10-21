//! 通知中心的存储与扫描
//!
//! 为什么不搞后台定时器：这是 local-first 的单用户桌面应用，控制台打开时才是"有人在看"。
//! 所以告警是**读时扫描**出来的——`GET /api/alerts` 先按当前状态算出候选，再与库里的行合并。
//! 表存的是"某个条件现在的状态 + 用户读到没有"，不是事件流水：
//! 反复扫到同一条件只移动 `last_seen`，条件消失就把行标成 resolved，重新出现才重新算未读。
//!
//! 阈值都写在这里，刻意不做成配置项：`docs/console-redesign.md` §6.7 只要求这几类聚合告警，
//! 提前做配置面是猜需求。真要调，改常量比改 UI 更快也更难被误配。

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::alert::*;

/// 平台最近 N 次连通检测全失败才告警。检测只由"测试连接"写入，
/// 所以最新一次是红的就说明用户最后一次看到的是挂掉的状态。
const PLATFORM_FAIL_STREAK: i64 = 3;
/// 同一虚拟模型最近 N 条请求全失败，且最新一条还在 24 小时内——三个月前的三次失败不该一直挂着
const MODEL_FAIL_STREAK: i64 = 3;
const MODEL_RECENT_WINDOW_HOURS: i64 = 24;
/// 15 分钟窗口内至少 5 条请求、失败率 ≥ 40% 才算"错误率超阈值"。
/// 样本太小的比例没有意义，所以有下限。
const ERROR_RATE_WINDOW_MINUTES: i64 = 15;
const ERROR_RATE_MIN_REQUESTS: i64 = 5;
const ERROR_RATE_THRESHOLD: f64 = 0.4;
/// Key 到期提醒窗口
const KEY_EXPIRY_WARNING_DAYS: i64 = 7;
/// 配额用到这个比例就提醒一次。"用满"那条只在事后说话，这条要提前一步。
const BUDGET_WARN_RATIO: f64 = 0.8;
/// 超过这么多天没有整库快照就提醒。备份是唯一能找回 `request_stats` 的手段
/// （配置另有 JSON 导出），但拍不拍、什么时候拍只有用户自己知道，所以只做提醒不做定时任务。
const BACKUP_STALE_DAYS: i64 = 7;
/// 已解决的告警保留 30 天后删除，否则这张表只增不减
const RESOLVED_RETENTION_DAYS: i64 = 30;
/// 面板一次最多列这么多条（未读优先），历史再多也不该把面板撑成长列表
const LIST_LIMIT: i64 = 60;

const ALERT_COLS: &str = "id, dedupe_key, kind, severity, ref_type, ref_id, code, args, first_seen, last_seen, read_at, resolved_at";

fn map_alert(r: &rusqlite::Row) -> rusqlite::Result<Alert> {
    let raw: String = r.get(7)?;
    // args 写进来的是 JSON 数组；解析失败就当单条参数，绝不让一个坏行把整个面板打挂
    let args: Vec<String> = serde_json::from_str(&raw).unwrap_or_else(|_| vec![raw]);
    Ok(Alert {
        id: r.get(0)?,
        dedupe_key: r.get(1)?,
        kind: r.get(2)?,
        severity: r.get(3)?,
        ref_type: r.get(4)?,
        ref_id: r.get(5)?,
        code: r.get(6)?,
        args,
        first_seen: r.get(8)?,
        last_seen: r.get(9)?,
        read_at: r.get(10)?,
        resolved_at: r.get(11)?,
    })
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 窗口起点：与 `db::stats` 的边界口径一致（UTC 秒精度、带 +00:00），
/// 这样才能和存成 `...SS.fff+00:00` 的 created_at 做字典序比较。
fn bound(minutes_back: i64) -> String {
    let at = chrono::Utc::now() - chrono::Duration::minutes(minutes_back);
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 到期窗口：`(已过期, 还剩几天)`。已过期不论差多久都直接算过期，
/// 未过期向上取整（6 小时内算 1 天）。解析不了的时间串按"不提醒"处理。
fn expiry_window(expires_at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<(bool, i64)> {
    let parsed = chrono::DateTime::parse_from_rfc3339(expires_at).ok()?;
    let secs = parsed.with_timezone(&chrono::Utc).signed_duration_since(now).num_seconds();
    if secs <= 0 {
        return Some((true, 0));
    }
    Some((false, (secs as f64 / 86400.0).ceil() as i64))
}

/// 错误率判定：样本够且超过阈值才返回百分比（整数部分）
fn error_rate_pct(total: i64, failed: i64) -> Option<i64> {
    if total < ERROR_RATE_MIN_REQUESTS {
        return None;
    }
    let rate = failed as f64 / total as f64;
    if rate >= ERROR_RATE_THRESHOLD {
        Some((rate * 100.0).round() as i64)
    } else {
        None
    }
}

/// 一把设了配额的 Key 加上它在各自窗口里的已用量。
/// 字段摊平成一个结构体是因为维度到了三个，六元组读起来已经不像代码了。
struct QuotaRow {
    id: String,
    name: String,
    quota_requests: Option<i64>,
    quota_tokens: Option<i64>,
    quota_cost: Option<f64>,
    used_requests: i64,
    used_tokens: i64,
    used_cost: f64,
    priced_requests: i64,
}

/// 金额进告警文案：精度跟着数量级走（与前端 `formatCost` 同一口径），尾随零由 `Display` 去掉。
/// 固定两位会把 0.00225 渲染成 `0`——"成本预算已用满（0/0）"等于没说事。
fn trim_money(value: f64) -> String {
    let abs = value.abs();
    let digits: u32 = if abs >= 100.0 {
        2
    } else if abs >= 1.0 {
        3
    } else if abs >= 0.01 {
        4
    } else {
        6
    };
    let factor = 10f64.powi(digits as i32);
    format!("{}", (value * factor).round() / factor)
}

/// 扫当前状态，得到候选告警。七个条件各自一条查询，全部走索引，控制台打开时顺手跑一次。
/// 唯一不进库的输入是"最近一份整库快照的时间"（unix 秒，`None` = 从没拍过）：
/// 那是文件系统上的事实，由调用方读好传进来，这里不碰 fs 才好测试。
pub fn collect_candidates(
    pool: &DbPool,
    now: chrono::DateTime<chrono::Utc>,
    latest_backup_at: Option<i64>,
) -> AppResult<Vec<AlertCandidate>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut out: Vec<AlertCandidate> = Vec::new();
    // 配额告警按每把 Key 自己的窗口取分子，起点与 `enforce_quota` 完全同一份算法
    let (day_start, week_start, month_start) = (
        crate::models::api_key::window_start(Some("day"), now),
        crate::models::api_key::window_start(Some("week"), now),
        crate::models::api_key::window_start(Some("month"), now),
    );

    // 1. 平台连续失败：最近 N 次检测全红
    let mut stmt = conn.prepare(
        "WITH ranked AS (
           SELECT platform_id, ok, ROW_NUMBER() OVER (PARTITION BY platform_id ORDER BY id DESC) AS rn
           FROM platform_health
         )
         SELECT p.id, p.name, COUNT(*)
         FROM ranked r JOIN platforms p ON p.id = r.platform_id
         WHERE r.rn <= ?1 AND r.ok = 0
         GROUP BY p.id, p.name
         HAVING COUNT(*) = ?1
         ORDER BY p.name",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![PLATFORM_FAIL_STREAK], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|x| x.ok())
        .collect();
    drop(stmt);
    for (id, name) in rows {
        out.push(AlertCandidate {
            dedupe_key: format!("platform_streak:{}", id),
            kind: "platform_streak",
            severity: "error",
            ref_type: Some("platform".into()),
            ref_id: Some(id.clone()),
            code: "alertPlatformStreak",
            args: vec![name, PLATFORM_FAIL_STREAK.to_string()],
        });
    }

    // 2. 模型故障：最近 N 条请求全失败且最新一条仍在窗口内
    let mut stmt = conn.prepare(
        "WITH ranked AS (
           SELECT proxy_id, status_code, created_at,
                  ROW_NUMBER() OVER (PARTITION BY proxy_id ORDER BY id DESC) AS rn
           FROM request_stats
         )
         SELECT p.id, p.name
         FROM ranked r JOIN proxies p ON p.id = r.proxy_id
         WHERE r.rn <= ?1 AND NOT (r.status_code >= 200 AND r.status_code < 300)
         GROUP BY p.id, p.name
         HAVING COUNT(*) = ?1 AND MAX(r.created_at) >= ?2
         ORDER BY p.name",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![MODEL_FAIL_STREAK, bound(MODEL_RECENT_WINDOW_HOURS * 60)], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|x| x.ok())
        .collect();
    drop(stmt);
    for (id, name) in rows {
        out.push(AlertCandidate {
            dedupe_key: format!("model_outage:{}", id),
            kind: "model_outage",
            severity: "error",
            ref_type: Some("proxy".into()),
            ref_id: Some(id.clone()),
            code: "alertModelOutage",
            args: vec![name, MODEL_FAIL_STREAK.to_string()],
        });
    }

    // 3. 全局错误率：失败口径与概览页一致（非 2xx 即失败）
    let (total, failed): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), SUM(CASE WHEN NOT (status_code >= 200 AND status_code < 300) THEN 1 ELSE 0 END)
         FROM request_stats WHERE created_at >= ?1",
        rusqlite::params![bound(ERROR_RATE_WINDOW_MINUTES)],
        |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
    )?;
    if let Some(pct) = error_rate_pct(total, failed) {
        out.push(AlertCandidate {
            dedupe_key: "error_rate".into(),
            kind: "error_rate",
            severity: "warn",
            ref_type: None,
            ref_id: None,
            code: "alertErrorRate",
            args: vec![pct.to_string(), total.to_string()],
        });
    }

    // 4. Key 即将过期 / 已过期：只提醒启用的 Key，停用的本就不该再被使用
    let mut stmt = conn.prepare("SELECT id, name, expires_at FROM api_keys WHERE enabled = 1 AND expires_at IS NOT NULL ORDER BY name")?;
    let keys: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|x| x.ok())
        .collect();
    drop(stmt);
    for (id, name, expires_at) in keys {
        let Some((expired, days)) = expiry_window(&expires_at, now) else { continue };
        if !expired && days > KEY_EXPIRY_WARNING_DAYS {
            continue;
        }
        let (code, args, severity) = if expired {
            ("alertKeyExpired", vec![name], "error")
        } else {
            ("alertKeyExpiring", vec![name, days.to_string()], "warn")
        };
        out.push(AlertCandidate {
            dedupe_key: format!("key_expiry:{}", id),
            kind: "key_expiry",
            severity,
            ref_type: Some("api_key".into()),
            ref_id: Some(id),
            code,
            args,
        });
    }

    // 5. Key 配额用满：分子与鉴权处的 enforce_quota 同源——同一份 request_stats，
    //    并且按每把 Key 自己的 quota_window 取时间下界，否则"告警说没满、请求却被拒"会同时成立。
    let mut stmt = conn.prepare(
        "SELECT k.id, k.name, k.quota_requests, k.quota_tokens, k.quota_cost,
                COUNT(s.id),
                COALESCE(SUM(COALESCE(s.token_input,0) + COALESCE(s.token_output,0)), 0),
                COALESCE(SUM(s.cost), 0), COUNT(s.cost)
         FROM api_keys k
         LEFT JOIN request_stats s ON s.api_key_id = k.id
           AND (k.quota_window IS NULL
                OR (k.quota_window='day'   AND s.created_at>=?1)
                OR (k.quota_window='week'  AND s.created_at>=?2)
                OR (k.quota_window='month' AND s.created_at>=?3))
         WHERE k.enabled = 1
           AND (k.quota_requests IS NOT NULL OR k.quota_tokens IS NOT NULL OR k.quota_cost IS NOT NULL)
         GROUP BY k.id
         ORDER BY k.name",
    )?;
    let rows = stmt.query_map(rusqlite::params![day_start, week_start, month_start], |r| {
        Ok(QuotaRow {
            id: r.get(0)?,
            name: r.get(1)?,
            quota_requests: r.get(2)?,
            quota_tokens: r.get(3)?,
            quota_cost: r.get(4)?,
            used_requests: r.get(5)?,
            used_tokens: r.get(6)?,
            used_cost: r.get(7)?,
            priced_requests: r.get(8)?,
        })
    })?;
    let quotas: Vec<QuotaRow> = rows.filter_map(|x| x.ok()).collect();
    drop(stmt);

    for row in quotas {
        // 金额只累加已定价的请求：一把只走未定价模型的 Key 算不出成本，
        // 就既不该报"预算用满"，也不该报"预算接近上限"。
        let mut hit: Option<(&str, Vec<String>)> = None;
        if let Some(max) = row.quota_requests {
            if row.used_requests >= max {
                hit = Some(("alertKeyQuota", vec![row.name.clone(), row.used_requests.to_string(), max.to_string()]));
            }
        }
        if hit.is_none() {
            if let Some(max) = row.quota_tokens {
                if row.used_tokens >= max {
                    hit = Some(("alertKeyQuota", vec![row.name.clone(), row.used_tokens.to_string(), max.to_string()]));
                }
            }
        }
        if hit.is_none() {
            if let Some(max) = row.quota_cost {
                if row.priced_requests > 0 && row.used_cost >= max {
                    hit = Some(("alertKeyQuotaCost", vec![row.name.clone(), trim_money(row.used_cost), trim_money(max)]));
                }
            }
        }
        if let Some((code, args)) = hit {
            out.push(AlertCandidate {
                dedupe_key: format!("key_quota:{}", row.id),
                kind: "key_quota",
                severity: "warn",
                ref_type: Some("api_key".into()),
                ref_id: Some(row.id.clone()),
                code,
                args,
            });
        }

        // 6. 接近上限：用满那条只在事后说话，这条提前一步。只报比例最高的那一维，
        //    一把 Key 同时逼近三个配额也只占一行——铃铛里三条一样的 Key 名读起来没有信息量。
        let dimensions: [(Option<f64>, &str); 3] = [
            (row.quota_requests.map(|max| row.used_requests as f64 / max as f64), "alertKeyBudgetRequests"),
            (row.quota_tokens.map(|max| row.used_tokens as f64 / max as f64), "alertKeyBudgetTokens"),
            (if row.priced_requests > 0 { row.quota_cost.map(|max| row.used_cost / max) } else { None }, "alertKeyBudgetCost"),
        ];
        let worst = dimensions
            .into_iter()
            .filter_map(|(ratio, code)| ratio.map(|r| (r, code)))
            .filter(|(ratio, _)| *ratio >= BUDGET_WARN_RATIO && *ratio < 1.0)
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((ratio, code)) = worst {
            out.push(AlertCandidate {
                dedupe_key: format!("key_budget:{}", row.id),
                kind: "key_budget",
                severity: "warn",
                ref_type: Some("api_key".into()),
                ref_id: Some(row.id.clone()),
                code,
                // 只给百分比：金额配额没有统一币种，报一个带币符的数字反而是编的
                args: vec![row.name.clone(), (ratio * 100.0).floor().to_string()],
            });
        }
    }

    // 7. 备份新鲜度：从没拍过、或者最后一次拍打太久以前，都提醒一次。
    //    时钟回拨得到的负差按 0 天处理，宁可少报也别报一个"负几天前"。
    let age_days = latest_backup_at.map(|secs| {
        let delta = now.timestamp() - secs;
        if delta < 0 {
            0
        } else {
            (delta as f64 / 86400.0).ceil() as i64
        }
    });
    let stale = match age_days {
        None => Some(("alertBackupNone", Vec::<String>::new())),
        Some(days) if days > BACKUP_STALE_DAYS => {
            Some(("alertBackupStale", vec![days.to_string()]))
        }
        Some(_) => None,
    };
    if let Some((code, args)) = stale {
        out.push(AlertCandidate {
            // 备份是全局一件事，没有可引用的行，所以键也是固定的
            dedupe_key: "backup_staleness".into(),
            kind: "backup_staleness",
            severity: "warn",
            ref_type: Some("settings".into()),
            ref_id: None,
            code,
            args,
        });
    }

    Ok(out)
}

/// 把候选状态并进表里：新出现的插入、还在的只移动 `last_seen`、
/// 扫不到的开放行标成已解决、已解决的保留期一到就删。
pub fn sweep(pool: &DbPool, candidates: &[AlertCandidate]) -> AppResult<AlertSweep> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let now = now_iso();
    let mut opened = 0usize;
    let mut resolved = 0usize;

    for c in candidates {
        let existing: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT read_at, resolved_at FROM alerts WHERE dedupe_key = ?1",
                rusqlite::params![c.dedupe_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let args = serde_json::to_string(&c.args)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        match existing {
            // 从没解决过：只是"还在"，移动 last_seen 并刷新参数（引用的名字可能被改掉）
            Some((_, None)) => {
                conn.execute(
                    "UPDATE alerts SET severity = ?2, ref_type = ?3, ref_id = ?4, code = ?5, args = ?6, last_seen = ?7
                     WHERE dedupe_key = ?1",
                    rusqlite::params![c.dedupe_key, c.severity, c.ref_type, c.ref_id, c.code, args, now],
                )?;
            }
            // 解决过又回来了：算新的一条，已读状态清零，否则用户永远看不到回归的故障
            Some((_, Some(_))) => {
                conn.execute(
                    "UPDATE alerts SET severity = ?2, ref_type = ?3, ref_id = ?4, code = ?5, args = ?6,
                            first_seen = ?7, last_seen = ?7, read_at = NULL, resolved_at = NULL
                     WHERE dedupe_key = ?1",
                    rusqlite::params![c.dedupe_key, c.severity, c.ref_type, c.ref_id, c.code, args, now],
                )?;
                opened += 1;
            }
            None => {
                conn.execute(
                    "INSERT INTO alerts (dedupe_key, kind, severity, ref_type, ref_id, code, args, first_seen, last_seen)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    rusqlite::params![
                        c.dedupe_key, c.kind, c.severity, c.ref_type, c.ref_id, c.code, args, now
                    ],
                )?;
                opened += 1;
            }
        }
    }

    // 条件不再成立 → 标记解决。用 NOT IN 而不是逐行判断，候选集本来就是内存里的十来个键。
    let open_keys: Vec<String> = conn
        .prepare("SELECT dedupe_key FROM alerts WHERE resolved_at IS NULL")?
        .query_map([], |r| r.get(0))?
        .filter_map(|x| x.ok())
        .collect();
    for key in open_keys {
        if !candidates.iter().any(|c| c.dedupe_key == key) {
            conn.execute(
                "UPDATE alerts SET resolved_at = ?2 WHERE dedupe_key = ?1",
                rusqlite::params![key, now],
            )?;
            resolved += 1;
        }
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(RESOLVED_RETENTION_DAYS);
    conn.execute(
        "DELETE FROM alerts WHERE resolved_at IS NOT NULL AND resolved_at <= ?1",
        rusqlite::params![cutoff.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)],
    )?;

    let unread: i64 = conn.query_row(
        "SELECT COUNT(*) FROM alerts WHERE read_at IS NULL AND resolved_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(AlertSweep { opened, resolved, unread: unread as usize })
}

/// 面板列表：未解决在前（error 优先、再按最近出现），已解决的垫后，最多 LIST_LIMIT 条
pub fn list(pool: &DbPool, include_resolved: bool) -> AppResult<Vec<Alert>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let sql = format!(
        "SELECT {} FROM alerts
         {}
         ORDER BY (resolved_at IS NOT NULL) ASC,
                  CASE WHEN severity = 'error' THEN 0 ELSE 1 END ASC,
                  last_seen DESC
         LIMIT ?1",
        ALERT_COLS,
        if include_resolved { "" } else { "WHERE resolved_at IS NULL" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![LIST_LIMIT], map_alert)?;
    Ok(rows.filter_map(|x| x.ok()).collect())
}

pub fn unread_count(pool: &DbPool) -> AppResult<i64> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM alerts WHERE read_at IS NULL AND resolved_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// 标记单条已读；返回是否命中（路径写错就该 404，不静默成功）
pub fn mark_read(pool: &DbPool, dedupe_key: &str) -> AppResult<bool> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let changed = conn.execute(
        "UPDATE alerts SET read_at = ?2 WHERE dedupe_key = ?1 AND read_at IS NULL",
        rusqlite::params![dedupe_key, now_iso()],
    )?;
    if changed > 0 {
        return Ok(true);
    }
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM alerts WHERE dedupe_key = ?1",
        rusqlite::params![dedupe_key],
        |r| r.get(0),
    )?;
    Ok(exists > 0)
}

pub fn mark_all_read(pool: &DbPool) -> AppResult<usize> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(conn.execute(
        "UPDATE alerts SET read_at = ?1 WHERE read_at IS NULL AND resolved_at IS NULL",
        rusqlite::params![now_iso()],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(key: &str, code: &'static str, args: Vec<String>, severity: &'static str) -> AlertCandidate {
        AlertCandidate {
            dedupe_key: key.into(),
            kind: "test",
            severity,
            ref_type: Some("platform".into()),
            ref_id: Some("p1".into()),
            code,
            args,
        }
    }

    fn temp_db(name: &str) -> (DbPool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("aigw-alert-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("test.db");
        let pool = crate::db::init_pool(&path).expect("init pool");
        (pool, path)
    }

    fn cleanup(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// 状态机：新出现→插入并算未读；还在→只移动 last_seen 且不重复打扰；
    /// 条件消失→标解决；解决后回归→重新算新告警并清空已读
    #[test]
    fn sweep_tracks_open_read_and_resolve() {
        let (pool, path) = temp_db("state");

        let first = sweep(&pool, &[candidate("a", "alertPlatformStreak", vec!["A".into()], "error")]).unwrap();
        assert_eq!(first.opened, 1);
        assert_eq!(first.unread, 1, "新告警默认未读");

        // 同样的条件再扫一次：不该多出一条，也不该把已读清掉
        mark_read(&pool, "a").unwrap();
        let again = sweep(&pool, &[candidate("a", "alertPlatformStreak", vec!["A".into()], "error")]).unwrap();
        assert_eq!(again.opened, 0, "还在成立的条件不算新告警");
        assert_eq!(again.unread, 0, "读过的不该因为刷新又变未读");
        assert_eq!(list(&pool, false).unwrap().len(), 1);

        // 参数会跟着状态刷新（平台改名、次数变化都要能反映）
        sweep(&pool, &[candidate("a", "alertPlatformStreak", vec!["B".into(), "5".into()], "warn")]).unwrap();
        let row = &list(&pool, false).unwrap()[0];
        assert_eq!(row.args, vec!["B".to_string(), "5".to_string()]);
        assert_eq!(row.severity, "warn");
        assert_eq!(row.ref_id.as_deref(), Some("p1"));

        // 条件消失 → 解决，列表里不再出现，但 include_resolved 还能翻到
        let cleared = sweep(&pool, &[]).unwrap();
        assert_eq!(cleared.resolved, 1);
        assert!(list(&pool, false).unwrap().is_empty());
        assert_eq!(list(&pool, true).unwrap().len(), 1);
        assert_eq!(unread_count(&pool).unwrap(), 0);

        // 回归：算新的一条，未读清零（否则用户看不见重新发生的故障）
        mark_read(&pool, "a").unwrap();
        let refired = sweep(&pool, &[candidate("a", "alertPlatformStreak", vec!["B".into()], "error")]).unwrap();
        assert_eq!(refired.opened, 1, "解决后又出现要重新提醒");
        assert_eq!(refired.unread, 1);

        // 全部已读：一次把未解决的全部标记
        mark_all_read(&pool).unwrap();
        assert_eq!(unread_count(&pool).unwrap(), 0);
        // 已读再点一次不该报错，也不该有变化
        assert_eq!(mark_all_read(&pool).unwrap(), 0);

        // 未知 key：mark_read 返回 false（接口据此 404），而不是静默成功
        assert!(!mark_read(&pool, "nope").unwrap());

        drop(pool);
        cleanup(&path);
    }

    /// 纯函数阈值：样本太少的错误率不告警；到期天数按向上取整，过了时间点就是已过期
    #[test]
    fn thresholds_are_bounded_by_sample_size() {
        assert_eq!(error_rate_pct(4, 4), None, "4 条全失败也不算错误率告警");
        assert_eq!(error_rate_pct(5, 1), None, "20% 不告警");
        assert_eq!(error_rate_pct(10, 4), Some(40), "刚好 40% 是阈值上，要告警");
        assert_eq!(error_rate_pct(10, 10), Some(100));

        let now = chrono::Utc::now();
        let in_6h = (now + chrono::Duration::hours(6)).to_rfc3339();
        assert_eq!(expiry_window(&in_6h, now), Some((false, 1)), "不足一天算一天，别说成 0 天后");
        let in_20d = (now + chrono::Duration::days(20)).to_rfc3339();
        assert_eq!(expiry_window(&in_20d, now), Some((false, 20)));
        let past = (now - chrono::Duration::hours(2)).to_rfc3339();
        assert_eq!(expiry_window(&past, now), Some((true, 0)), "过期 2 小时也是过期，不是还剩 0 天");
        assert_eq!(expiry_window("not-a-date", now), None, "解析不了就按不提醒，不猜");
    }

    /// 检测器：只在条件真的成立时开口，健康平台 / 只有一次失败的模型不该出现
    #[test]
    fn detectors_only_fire_on_actual_state() {
        let (pool, path) = temp_db("detectors");
        let conn = pool.get().unwrap();
        let recent = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        let old = (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

        conn.execute("INSERT INTO platforms (id, name, type, base_url, api_key, created_at, updated_at) VALUES ('pl-bad','坏平台','openai','http://x','k',?1,?1)", rusqlite::params![recent]).unwrap();
        conn.execute("INSERT INTO platforms (id, name, type, base_url, api_key, created_at, updated_at) VALUES ('pl-ok','好平台','openai','http://y','k',?1,?1)", rusqlite::params![recent]).unwrap();
        for _ in 0..3 {
            conn.execute("INSERT INTO platform_health (platform_id, ok, latency_ms, model_count, error, created_at) VALUES ('pl-bad',0,1,0,'连接被拒绝',?1)", rusqlite::params![recent]).unwrap();
        }
        conn.execute("INSERT INTO platform_health (platform_id, ok, latency_ms, model_count, error, created_at) VALUES ('pl-ok',1,5,2,NULL,?1)", rusqlite::params![recent]).unwrap();
        // 两次失败的模型：没到 3 次不该告警
        conn.execute("INSERT INTO proxies (id, name, created_at) VALUES ('px-flaky','抖动模型',?1)", rusqlite::params![recent]).unwrap();
        for _ in 0..2 {
            conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at) VALUES ('px-flaky','r','b',502,10,NULL,NULL,?1)", rusqlite::params![recent]).unwrap();
        }
        // 三次失败但都是 3 天前：陈旧故障不该一直挂着
        conn.execute("INSERT INTO proxies (id, name, created_at) VALUES ('px-stale','陈旧模型',?1)", rusqlite::params![recent]).unwrap();
        for _ in 0..3 {
            conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at) VALUES ('px-stale','r','b',500,10,NULL,NULL,?1)", rusqlite::params![old]).unwrap();
        }
        conn.execute("INSERT INTO proxies (id, name, created_at) VALUES ('px-down','故障模型',?1)", rusqlite::params![recent]).unwrap();
        for _ in 0..3 {
            conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at) VALUES ('px-down','r','b',503,10,NULL,NULL,?1)", rusqlite::params![recent]).unwrap();
        }

        let keys = || -> Vec<String> {
            collect_candidates(&pool, chrono::Utc::now(), None)
                .unwrap()
                .into_iter()
                .map(|c| c.dedupe_key)
                .collect()
        };
        let fired = keys();
        assert!(fired.contains(&"platform_streak:pl-bad".to_string()), "连续三次失败要告警：{:?}", fired);
        assert!(!fired.contains(&"platform_streak:pl-ok".to_string()));
        assert!(fired.contains(&"model_outage:px-down".to_string()));
        assert!(!fired.contains(&"model_outage:px-flaky".to_string()), "2 次不到阈值");
        assert!(!fired.contains(&"model_outage:px-stale".to_string()), "窗口外的旧故障不算当前状态");
        // px-down 3 条 + px-flaky 2 条 = 5 条样本，全在窗口内且全失败 → 错误率条件同时成立
        assert!(fired.contains(&"error_rate".to_string()), "15 分钟内 5 条全失败要报错误率：{:?}", fired);

        // Key：即将过期 + 配额用满各一条；健康 Key 不该出现
        let soon = (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339();
        let far = (chrono::Utc::now() + chrono::Duration::days(60)).to_rfc3339();
        conn.execute("INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled, expires_at, quota_requests, quota_tokens) VALUES ('k-soon','快到期','h1','****1',?1,1,?2,NULL,NULL)", rusqlite::params![recent, soon]).unwrap();
        conn.execute("INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled, expires_at, quota_requests, quota_tokens) VALUES ('k-far','早着呢','h2','****2',?1,1,?2,NULL,NULL)", rusqlite::params![recent, far]).unwrap();
        conn.execute("INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled, expires_at, quota_requests, quota_tokens) VALUES ('k-quota','配额满','h3','****3',?1,1,NULL,2,NULL)", rusqlite::params![recent]).unwrap();
        conn.execute("INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled) VALUES ('k-clean','干净','h4','****4',?1,1)", rusqlite::params![recent]).unwrap();
        let lapsed = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        conn.execute("INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled, expires_at, quota_requests, quota_tokens) VALUES ('k-lapsed','刚过期','h5','****5',?1,1,?2,NULL,NULL)", rusqlite::params![recent, lapsed]).unwrap();
        for _ in 0..2 {
            conn.execute("INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at, api_key_id) VALUES ('px-down','r','b',200,10,1,1,?1,'k-quota')", rusqlite::params![recent]).unwrap();
        }

        let fired = keys();
        assert!(fired.contains(&"key_expiry:k-soon".to_string()));
        assert!(!fired.contains(&"key_expiry:k-far".to_string()), "60 天后到期不该打扰");
        assert!(!fired.contains(&"key_expiry:k-clean".to_string()));
        assert!(fired.contains(&"key_expiry:k-lapsed".to_string()), "过期 2 小时也该挂着");
        assert!(fired.contains(&"key_quota:k-quota".to_string()), "用满配额要提醒：{:?}", fired);

        let codes: Vec<(String, String)> = collect_candidates(&pool, chrono::Utc::now(), None)
            .unwrap()
            .into_iter()
            .map(|c| (c.dedupe_key, c.code.to_string()))
            .collect();
        let code_of = |key: &str| codes.iter().find(|(d, _)| d == key).map(|(_, c)| c.as_str());
        assert_eq!(code_of("key_expiry:k-lapsed"), Some("alertKeyExpired"), "已过期不能报成\"还剩 0 天\"");
        assert_eq!(code_of("key_expiry:k-soon"), Some("alertKeyExpiring"));

        drop(pool);
        cleanup(&path);
    }

    /// 成本预算与"接近上限"：分子按每把 Key 自己的窗口取，未定价的请求不参与金额判定。
    #[test]
    fn cost_and_budget_detectors_follow_the_same_numerator_as_the_gate() {
        let (pool, path) = temp_db("budget");
        let conn = pool.get().unwrap();
        let recent = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        let old = (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

        let mut key = |id: &str, quota_cost: Option<f64>, quota_requests: Option<i64>, window: Option<&str>| {
            conn.execute(
                "INSERT INTO api_keys (id, name, key_hash, key_mask, created_at, enabled, quota_requests, quota_cost, quota_window) VALUES (?1,?1,?1,'****',?2,1,?3,?4,?5)",
                rusqlite::params![id, recent, quota_requests, quota_cost, window],
            ).unwrap();
        };
        let mut stat = |id: &str, cost: Option<f64>, at: &str| {
            conn.execute(
                "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at, api_key_id, cost) VALUES ('p','r','b',200,10,1,1,?1,?2,?3)",
                rusqlite::params![at, id, cost],
            ).unwrap();
        };

        key("k-cost", Some(2.0), None, None);
        stat("k-cost", Some(2.5), &recent);
        // 小额预算：按 token 计价时预算常常不到一分，文案不能塌成 0/0
        key("k-cents", Some(0.00225), None, None);
        stat("k-cents", Some(0.00225), &recent);
        key("k-near", None, Some(10), None);
        for _ in 0..8 {
            stat("k-near", None, &recent);
        }
        // 只走未定价模型：算不出成本，就不该报预算
        key("k-unpriced", Some(2.0), None, None);
        for _ in 0..4 {
            stat("k-unpriced", None, &recent);
        }
        // 日窗口：4 条都在窗口外，窗口内只有 1/5 → 既不满也不接近
        key("k-window", None, Some(5), Some("day"));
        for _ in 0..4 {
            stat("k-window", None, &old);
        }
        stat("k-window", None, &recent);

        let candidates = collect_candidates(&pool, chrono::Utc::now(), None).unwrap();
        let found = |dedupe: &str| candidates.iter().find(|c| c.dedupe_key == dedupe);

        let exhausted = found("key_quota:k-cost").expect("成本用满要报");
        assert_eq!(exhausted.code, "alertKeyQuotaCost");
        assert_eq!(exhausted.args, vec!["k-cost", "2.5", "2"]);

        let cents = found("key_quota:k-cents").expect("小额预算用满同样要报");
        assert_eq!(cents.args, vec!["k-cents", "0.00225", "0.00225"]);

        let near = found("key_budget:k-near").expect("用到 80% 要提前提醒");
        assert_eq!(near.code, "alertKeyBudgetRequests");
        assert_eq!(near.args, vec!["k-near", "80"]);

        assert!(found("key_quota:k-unpriced").is_none(), "算不出成本就不报金额配额");
        assert!(found("key_budget:k-unpriced").is_none());
        assert!(found("key_quota:k-window").is_none(), "窗口外的用量不算当前状态：{:?}", candidates.iter().map(|c| &c.dedupe_key).collect::<Vec<_>>());
        assert!(found("key_budget:k-window").is_none());

        drop(pool);
        cleanup(&path);
    }

    /// 备份新鲜度是唯一不吃库里行的条件：它只看调用方递进来的时间戳
    #[test]
    fn backup_freshness_uses_the_filesystem_input_only() {
        let (pool, path) = temp_db("backup");
        let now = chrono::Utc::now();
        let at = |days: i64| (now - chrono::Duration::days(days)).timestamp();
        let staleness = |latest: Option<i64>| {
            collect_candidates(&pool, now, latest)
                .unwrap()
                .into_iter()
                .filter(|c| c.kind == "backup_staleness")
                .map(|c| (c.code, c.args, c.severity))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            staleness(None),
            vec![("alertBackupNone", vec![], "warn")],
            "从没拍过快照也要提醒一次"
        );
        assert!(staleness(Some(at(0))).is_empty(), "刚拍过不该打扰");
        assert!(
            staleness(Some(at(BACKUP_STALE_DAYS))).is_empty(),
            "正好卡在窗口边缘不算过期"
        );
        assert!(
            staleness(Some(now.timestamp() + 3600)).is_empty(),
            "时钟回拨不能报成负数天"
        );

        let stale = staleness(Some(at(BACKUP_STALE_DAYS + 3)));
        assert_eq!(stale.len(), 1, "一条条件只该有一行");
        assert_eq!(stale[0].0, "alertBackupStale");
        assert_eq!(
            stale[0].1,
            vec![(BACKUP_STALE_DAYS + 3).to_string()],
            "隔了几天要能进文案"
        );

        drop(pool);
        cleanup(&path);
    }
}
