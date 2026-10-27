use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use rusqlite::TransactionBehavior;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::credit::PlatformCreditUsage;

/// 当前窗口读数（来自 platform_credit_state）。period_start 与当前窗口不一致时
/// 调用方按 0 处理；真正的归零发生在下一次 try_reserve/adjust 的写事务里。
#[derive(Debug, Clone)]
pub struct CreditState {
    pub platform_id: String,
    pub period_start: String,
    pub spent: f64,
}

/// 预留结果。Exhausted 带上已用与预算，调用方可以给出“250/250，何时重置”的明确错误。
#[derive(Debug, Clone, PartialEq)]
pub enum CreditReserve {
    NotMetered,
    Free,
    Reserved { charged: f64 },
    Exhausted { spent: f64, budget: f64 },
}

pub const CREDIT_ADJUST_LIMIT: f64 = 1_000_000.0;

fn period_string(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

fn local_midnight(date: NaiveDate) -> Option<DateTime<Utc>> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    Local.from_local_datetime(&naive).earliest().map(|dt| dt.with_timezone(&Utc))
}

fn period_start_at(window: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let today = now.date_naive();
    let date = match window {
        "day" => today,
        "week" => today - Duration::days(today.weekday().num_days_from_monday() as i64),
        "month" => NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?,
        _ => return None,
    };
    local_midnight(date)
}

fn next_period_start_at(window: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let today = now.date_naive();
    let next = match window {
        "day" => today + Duration::days(1),
        "week" => {
            let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            monday + Duration::days(7)
        }
        "month" => {
            let (year, month) = if today.month() == 12 {
                (today.year() + 1, 1)
            } else {
                (today.year(), today.month() + 1)
            };
            NaiveDate::from_ymd_opt(year, month, 1)?
        }
        _ => return None,
    };
    local_midnight(next)
}

pub fn current_period_start(window: &str) -> Option<String> {
    period_start_at(window, Local::now()).map(period_string)
}

pub fn next_reset_at(window: &str) -> Option<String> {
    next_period_start_at(window, Local::now()).map(period_string)
}

/// 读全部平台状态。热路径只做一次 SELECT，不逐平台查。
pub fn state_map(pool: &DbPool) -> AppResult<HashMap<String, CreditState>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT platform_id, period_start, spent FROM platform_credit_state",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CreditState {
            platform_id: row.get(0)?,
            period_start: row.get(1)?,
            spent: row.get(2)?,
        })
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let state = row?;
        out.insert(state.platform_id.clone(), state);
    }
    Ok(out)
}

/// 快照里的有效已用量：窗口已过期就按 0，不触发写。
pub fn effective_spent(
    map: &HashMap<String, CreditState>,
    platform_id: &str,
    window: &str,
) -> f64 {
    let Some(state) = map.get(platform_id) else { return 0.0 };
    match current_period_start(window) {
        Some(current) if state.period_start == current => state.spent,
        _ => 0.0,
    }
}

fn ensure_state(tx: &rusqlite::Transaction<'_>, platform_id: &str, period_start: &str, now: &str) -> AppResult<(String, f64)> {
    tx.execute(
        "INSERT OR IGNORE INTO platform_credit_state (platform_id, period_start, spent, updated_at)
         VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![platform_id, period_start, now],
    )?;
    let (stored_period, spent): (String, f64) = tx.query_row(
        "SELECT period_start, spent FROM platform_credit_state WHERE platform_id = ?1",
        [platform_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((stored_period, spent))
}

fn insert_ledger(
    tx: &rusqlite::Transaction<'_>,
    platform_id: &str,
    request_id: Option<&str>,
    model_id: Option<&str>,
    delta: f64,
    kind: &str,
    note: Option<&str>,
    now: &str,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO platform_credit_ledger (platform_id, request_id, model_id, delta, kind, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![platform_id, request_id, model_id, delta, kind, note, now],
    )?;
    Ok(())
}

/// 预留一次请求的积分。预算/窗口不完整时按“不计费”放行，并打 warning——
/// 配置写坏不应该让整条转发链路 500。
pub fn try_reserve(
    pool: &DbPool,
    platform_id: &str,
    budget: Option<f64>,
    window: Option<&str>,
    price: Option<f64>,
    request_id: &str,
    model_id: &str,
) -> AppResult<CreditReserve> {
    let Some(budget) = budget else { return Ok(CreditReserve::NotMetered) };
    let Some(window) = window else {
        tracing::warn!("平台 {} 配了积分预算但没有窗口，按不计费处理", platform_id);
        return Ok(CreditReserve::NotMetered);
    };
    let price = price.unwrap_or(0.0);
    if !budget.is_finite() || budget <= 0.0 || !price.is_finite() || price < 0.0 {
        tracing::warn!("平台 {} 的积分配置无效（budget={}, price={}），按不计费处理", platform_id, budget, price);
        return Ok(CreditReserve::NotMetered);
    }
    if price == 0.0 {
        return Ok(CreditReserve::Free);
    }
    let Some(period) = current_period_start(window) else {
        tracing::warn!("平台 {} 的积分窗口 {} 无法识别，按不计费处理", platform_id, window);
        return Ok(CreditReserve::NotMetered);
    };
    let now = chrono::Utc::now().to_rfc3339();

    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (stored_period, mut spent) = ensure_state(&tx, platform_id, &period, &now)?;
    if stored_period != period {
        spent = 0.0;
        tx.execute(
            "UPDATE platform_credit_state SET period_start = ?2, spent = 0, updated_at = ?3 WHERE platform_id = ?1",
            rusqlite::params![platform_id, period, now],
        )?;
    }
    if spent + price > budget + f64::EPSILON {
        // 事务在这里直接由 drop 回滚；上一句的归零也不落库，下一次仍会重试。
        return Ok(CreditReserve::Exhausted { spent, budget });
    }
    tx.execute(
        "UPDATE platform_credit_state SET spent = ?2, updated_at = ?3 WHERE platform_id = ?1",
        rusqlite::params![platform_id, spent + price, now],
    )?;
    insert_ledger(&tx, platform_id, Some(request_id), Some(model_id), price, "charge", None, &now)?;
    tx.commit()?;
    Ok(CreditReserve::Reserved { charged: price })
}

/// 失败释放。跨周期时不再把旧额度退进新周期，只留一条 note 流水。
pub fn release(
    pool: &DbPool,
    platform_id: &str,
    window: Option<&str>,
    price: f64,
    request_id: &str,
    model_id: &str,
) -> AppResult<()> {
    if !price.is_finite() || price <= 0.0 {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let row: Option<(String, f64)> = tx
        .query_row(
            "SELECT period_start, spent FROM platform_credit_state WHERE platform_id = ?1",
            [platform_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((stored_period, spent)) = row else {
        insert_ledger(&tx, platform_id, Some(request_id), Some(model_id), 0.0, "release", Some("state missing"), &now)?;
        tx.commit()?;
        return Ok(());
    };
    // 用窗口起点判断：能算出当前窗口且与状态一致才回退，否则视为跨周期。
    let same_period = window
        .and_then(current_period_start)
        .map(|current| current == stored_period)
        .unwrap_or(false);
    if !same_period {
        insert_ledger(&tx, platform_id, Some(request_id), Some(model_id), 0.0, "release", Some("cross-period"), &now)?;
        tx.commit()?;
        return Ok(());
    }
    let next = (spent - price).max(0.0);
    tx.execute(
        "UPDATE platform_credit_state SET spent = ?2, updated_at = ?3 WHERE platform_id = ?1",
        rusqlite::params![platform_id, next, now],
    )?;
    insert_ledger(&tx, platform_id, Some(request_id), Some(model_id), next - spent, "release", None, &now)?;
    tx.commit()?;
    Ok(())
}

/// 手动校准。delta 是“想补/想冲减多少”，实际变动按 spent 下限 0 截断后记账。
pub fn adjust(pool: &DbPool, platform_id: &str, delta: f64, note: Option<&str>) -> AppResult<(f64, f64)> {
    if !delta.is_finite() || delta == 0.0 || delta.abs() > CREDIT_ADJUST_LIMIT {
        return Err(AppError::BadRequest("delta must be non-zero and within ±1000000".to_string()));
    }
    let window: String = pool
        .get()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .query_row("SELECT credit_window FROM platforms WHERE id = ?1", [platform_id], |r| r.get(0))
        .map_err(|_| AppError::BadRequest("platform has no credit budget".to_string()))?;
    let period = current_period_start(&window)
        .ok_or_else(|| AppError::Internal("invalid credit window".to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (stored_period, spent) = ensure_state(&tx, platform_id, &period, &now)?;
    let spent = if stored_period != period { 0.0 } else { spent };
    let next = (spent + delta).max(0.0);
    tx.execute(
        "UPDATE platform_credit_state SET period_start = ?2, spent = ?3, updated_at = ?4 WHERE platform_id = ?1",
        rusqlite::params![platform_id, period, next, now],
    )?;
    insert_ledger(&tx, platform_id, None, None, next - spent, "adjust", note, &now)?;
    tx.commit()?;
    Ok((spent, next))
}

/// 用上游的权威余额校准本地台账。
///
/// 为什么需要它：本地台账是「本地扣减」模型，上游有权威余额，两者会漂移
/// （并发预留、失败释放、上游计费口径差异）。热路径不能用上游接口（每个请求
/// 多一次往返），所以由后台任务/手动按钮定期校准。
///
/// `upstream_remaining` 是上游报告的**剩余**额度。换算成本地口径：
/// `target_spent = budget - upstream_remaining`，再与当前 `spent` 比对。
///
/// 返回 `(old_spent, new_spent, delta)`；偏差在 ±1 以内不动（避免抖动）。
pub fn calibrate(
    pool: &DbPool,
    platform_id: &str,
    budget: f64,
    window: &str,
    upstream_remaining: f64,
) -> AppResult<(f64, f64, f64)> {
    if !upstream_remaining.is_finite() || upstream_remaining < 0.0 {
        return Err(AppError::BadRequest(
            "upstream_remaining must be a non-negative number".to_string(),
        ));
    }
    let period = current_period_start(window)
        .ok_or_else(|| AppError::BadRequest("invalid credit window".to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (stored_period, spent) = ensure_state(&tx, platform_id, &period, &now)?;
    // 跨周期时本地已归零，直接以本次校准为准
    let spent = if stored_period != period { 0.0 } else { spent };

    let target = (budget - upstream_remaining).max(0.0);
    let delta = target - spent;
    if delta.abs() <= 1.0 {
        tx.commit()?;
        return Ok((spent, spent, 0.0));
    }

    tx.execute(
        "UPDATE platform_credit_state SET period_start = ?2, spent = ?3, updated_at = ?4 WHERE platform_id = ?1",
        rusqlite::params![platform_id, period, target, now],
    )?;
    let note = format!(
        "上游剩余 {:.2} / 预算 {:.2}；本地原已用 {:.2} → 校准为 {:.2}",
        upstream_remaining, budget, spent, target
    );
    insert_ledger(&tx, platform_id, None, None, delta, "calibrate", Some(&note), &now)?;
    tx.commit()?;
    Ok((spent, target, delta))
}

/// 手动重置当前窗口：把已用清零并写一条 reset 流水。
pub fn reset(pool: &DbPool, platform_id: &str) -> AppResult<f64> {
    let window: String = pool
        .get()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .query_row("SELECT credit_window FROM platforms WHERE id = ?1", [platform_id], |r| r.get(0))
        .map_err(|_| AppError::BadRequest("platform has no credit budget".to_string()))?;
    let period = current_period_start(&window)
        .ok_or_else(|| AppError::Internal("invalid credit window".to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (stored_period, spent) = ensure_state(&tx, platform_id, &period, &now)?;
    let spent = if stored_period != period { 0.0 } else { spent };
    tx.execute(
        "UPDATE platform_credit_state SET period_start = ?2, spent = 0, updated_at = ?3 WHERE platform_id = ?1",
        rusqlite::params![platform_id, period, now],
    )?;
    insert_ledger(&tx, platform_id, None, None, -spent, "reset", Some("manual reset"), &now)?;
    tx.commit()?;
    Ok(spent)
}

/// 平台积分读数：只包含启用预算的平台。窗口无法识别或状态缺失时 spent 按 0。
pub fn usage_all(pool: &DbPool) -> AppResult<Vec<PlatformCreditUsage>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.credit_budget, p.credit_window,
                COALESCE(s.period_start, ''), COALESCE(s.spent, 0),
                (SELECT COUNT(*) FROM models m WHERE m.platform_id = p.id AND m.credit_price IS NULL)
         FROM platforms p
         LEFT JOIN platform_credit_state s ON s.platform_id = p.id
         WHERE p.credit_budget IS NOT NULL
         ORDER BY p.name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (platform_id, name, budget, window, stored_period, stored_spent, unpriced) = row?;
        let Some(window) = window else { continue };
        let Some(period) = current_period_start(&window) else { continue };
        let Some(reset_at) = next_reset_at(&window) else { continue };
        let spent = if stored_period == period { stored_spent } else { 0.0 };
        let remaining = (budget - spent).max(0.0);
        out.push(PlatformCreditUsage {
            platform_id,
            platform_name: name,
            budget,
            window,
            spent,
            remaining,
            period_start: period,
            reset_at,
            unpriced_models: unpriced,
            exhausted: remaining <= 0.0,
        });
    }
    Ok(out)
}

/// 台账保留：只清积分流水，不动当前状态。返回删除行数。
pub fn cleanup_ledger_before(pool: &DbPool, cutoff: &str) -> AppResult<i64> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let deleted = conn.execute(
        "DELETE FROM platform_credit_ledger WHERE created_at <= ?1",
        [cutoff],
    )?;
    Ok(deleted as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("credit-test-{}.db", uuid::Uuid::new_v4()));
        (crate::db::init_pool(&path).expect("init test db"), path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn seed_platform(pool: &DbPool, budget: f64, window: &str) -> String {
        crate::db::platform::create(pool, &crate::models::platform::CreatePlatformRequest {
            name: "credit-platform".into(),
            platform_type: crate::models::platform::PlatformType::OpenAI,
            base_url: "http://127.0.0.1:1/v1".into(),
            api_key: "sk-test".into(),
            organization: None,
            custom_headers: serde_json::Value::Null,
            credit_budget: Some(budget),
            credit_window: Some(window.into()),
            // 积分台账只在 credit_unit=credits 时生效，所以这里必须显式声明，
            // 否则 validate_credit_unit 会因为「带预算但不是 credits」而拒绝。
            credit_unit: Some("credits".into()),
            adapter: None,
        })
        .expect("create platform")
        .id
    }

    #[test]
    fn period_boundaries_are_local_natural_windows() {
        let now = Local::now();
        let day = current_period_start("day").expect("day");
        let day_dt = DateTime::parse_from_rfc3339(&day).unwrap();
        assert_eq!(day_dt.with_timezone(&Local).time(), chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());

        let week = current_period_start("week").expect("week");
        let week_local = DateTime::parse_from_rfc3339(&week).unwrap().with_timezone(&Local);
        assert_eq!(week_local.weekday(), chrono::Weekday::Mon);
        assert_eq!(week_local.time(), chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());

        let month = current_period_start("month").expect("month");
        assert_eq!(DateTime::parse_from_rfc3339(&month).unwrap().with_timezone(&Local).day(), 1);
        assert!(next_reset_at("day").is_some());
        assert_eq!(current_period_start("year"), None);
        let _ = now;
    }

    #[test]
    fn reserve_release_adjust_and_reset_share_one_ledger() {
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 5.0, "day");

        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), Some(2.0), "r1", "m1").unwrap(),
            CreditReserve::Reserved { charged: 2.0 }
        );
        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), Some(2.0), "r2", "m1").unwrap(),
            CreditReserve::Reserved { charged: 2.0 }
        );
        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), Some(2.0), "r3", "m1").unwrap(),
            CreditReserve::Exhausted { spent: 4.0, budget: 5.0 }
        );

        release(&pool, &id, Some("day"), 2.0, "r2", "m1").unwrap();
        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), Some(2.0), "r4", "m1").unwrap(),
            CreditReserve::Reserved { charged: 2.0 }
        );

        let usage = usage_all(&pool).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].spent, 4.0);
        assert_eq!(usage[0].remaining, 1.0);
        assert!(!usage[0].exhausted);

        adjust(&pool, &id, -1.0, Some("上游对账")).unwrap();
        assert_eq!(usage_all(&pool).unwrap()[0].spent, 3.0);
        reset(&pool, &id).unwrap();
        assert_eq!(usage_all(&pool).unwrap()[0].spent, 0.0);

        let conn = pool.get().unwrap();
        let kinds: Vec<String> = conn
            .prepare("SELECT kind FROM platform_credit_ledger ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|x| x.ok())
            .collect();
        assert_eq!(kinds, vec!["charge", "charge", "release", "charge", "adjust", "reset"]);

        cleanup_db(&path);
    }

    #[test]
    fn stale_period_resets_before_charging() {
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 5.0, "day");
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO platform_credit_state (platform_id, period_start, spent, updated_at) VALUES (?1, '2000-01-01T00:00:00+00:00', 99, '2000-01-01T00:00:00+00:00')",
            [&id],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), Some(2.0), "r1", "m1").unwrap(),
            CreditReserve::Reserved { charged: 2.0 }
        );
        let usage = usage_all(&pool).unwrap();
        assert_eq!(usage[0].spent, 2.0);
        cleanup_db(&path);
    }

    #[test]
    fn calibrate_aligns_local_spent_with_upstream_balance() {
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 100.0, "day");

        // 本地先扣 10，模拟若干次请求
        try_reserve(&pool, &id, Some(100.0), Some("day"), Some(10.0), "r1", "m1").unwrap();
        assert_eq!(usage_all(&pool).unwrap()[0].spent, 10.0);

        // 上游说只剩 70 → 实际已用 30，本地应该被校准到 30
        let (old, new, delta) = calibrate(&pool, &id, 100.0, "day", 70.0).unwrap();
        assert_eq!(old, 10.0);
        assert_eq!(new, 30.0);
        assert_eq!(delta, 20.0);
        assert_eq!(usage_all(&pool).unwrap()[0].spent, 30.0);

        // 台账里必须留下 calibrate 流水
        let conn = pool.get().unwrap();
        let kinds: Vec<String> = conn
            .prepare("SELECT kind FROM platform_credit_ledger ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(kinds, vec!["charge", "calibrate"]);
        drop(conn);

        cleanup_db(&path);
    }

    #[test]
    fn calibrate_ignores_tiny_drift() {
        // 偏差 ≤ 1 不动，避免上游舍入导致台账抖动
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 100.0, "day");
        try_reserve(&pool, &id, Some(100.0), Some("day"), Some(10.0), "r1", "m1").unwrap();

        let (old, new, delta) = calibrate(&pool, &id, 100.0, "day", 89.5).unwrap();
        assert_eq!(delta, 0.0, "0.5 的偏差不该触发校准");
        assert_eq!(old, new);
        assert_eq!(usage_all(&pool).unwrap()[0].spent, 10.0);

        cleanup_db(&path);
    }

    #[test]
    fn calibrate_rejects_negative_remaining() {
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 100.0, "day");
        assert!(calibrate(&pool, &id, 100.0, "day", -5.0).is_err());
        assert!(calibrate(&pool, &id, 100.0, "day", f64::NAN).is_err());
        cleanup_db(&path);
    }

    #[test]
    fn calibrate_never_goes_negative_on_spent() {
        // 上游报的剩余比预算还多（额度被补发）→ spent 应该夹到 0，不能为负
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 100.0, "day");
        let (_, new, _) = calibrate(&pool, &id, 100.0, "day", 150.0).unwrap();
        assert_eq!(new, 0.0);
        cleanup_db(&path);
    }

    #[test]
    fn not_metered_and_free_do_not_write_state() {
        let (pool, path) = test_db();
        let id = seed_platform(&pool, 5.0, "day");
        assert_eq!(
            try_reserve(&pool, &id, None, None, Some(2.0), "r1", "m1").unwrap(),
            CreditReserve::NotMetered
        );
        assert_eq!(
            try_reserve(&pool, &id, Some(5.0), Some("day"), None, "r2", "m1").unwrap(),
            CreditReserve::Free
        );
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM platform_credit_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        cleanup_db(&path);
    }
}
