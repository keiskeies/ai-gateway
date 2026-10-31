use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use rusqlite::OptionalExtension;

/// 一个平台账号的每日领取登录态。平台即账号：`platform_id` 唯一，
/// 删平台时这一行跟着删；`cookies_enc` 是整条 Cookie 头的密文，明文只在进程内存在。
#[derive(Debug, Clone)]
pub struct CheckinAccountRow {
    pub id: String,
    pub platform_id: String,
    pub platform: String,
    pub identity: String,
    pub label: String,
    pub cookies_enc: String,
    pub cookie_names: String,
    pub status: String,
    pub last_message: String,
    pub last_run_at: Option<String>,
    pub last_run_day: Option<String>,
    pub last_ok_day: Option<String>,
    pub balance: Option<f64>,
    pub daily_amount: Option<f64>,
    pub daily_done: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 一次运行的落库结果。`run_day` / `ok_day` 是本地日期（`YYYY-MM-DD`），
/// 后台线程据此判断「今天是否已经跑过」。
pub struct RunUpdate<'a> {
    pub status: &'a str,
    pub message: &'a str,
    pub run_at: &'a str,
    pub run_day: &'a str,
    pub ok_day: Option<&'a str>,
    pub balance: Option<f64>,
    pub daily_amount: Option<f64>,
    /// `None` = 这次没拿到有效数据（登录失效/网络错误），保留上次结果
    pub daily_done: Option<bool>,
}

const COLS: &str = "id, platform_id, platform, identity, label, cookies_enc, cookie_names, status, \
                    last_message, last_run_at, last_run_day, last_ok_day, balance, daily_amount, \
                    daily_done, created_at, updated_at";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<CheckinAccountRow> {
    Ok(CheckinAccountRow {
        id: row.get(0)?,
        platform_id: row.get(1)?,
        platform: row.get(2)?,
        identity: row.get(3)?,
        label: row.get(4)?,
        cookies_enc: row.get(5)?,
        cookie_names: row.get(6)?,
        status: row.get(7)?,
        last_message: row.get(8)?,
        last_run_at: row.get(9)?,
        last_run_day: row.get(10)?,
        last_ok_day: row.get(11)?,
        balance: row.get(12)?,
        daily_amount: row.get(13)?,
        daily_done: row.get::<_, i64>(14)? != 0,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

pub fn list(pool: &DbPool) -> AppResult<Vec<CheckinAccountRow>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM checkin_accounts ORDER BY created_at"))?;
    let rows = stmt.query_map([], map_row)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Option<CheckinAccountRow>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM checkin_accounts WHERE id = ?1"),
            rusqlite::params![id],
            map_row,
        )
        .optional()?)
}

pub fn get_by_platform(pool: &DbPool, platform_id: &str) -> AppResult<Option<CheckinAccountRow>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM checkin_accounts WHERE platform_id = ?1"),
            rusqlite::params![platform_id],
            map_row,
        )
        .optional()?)
}

/// 写入/覆盖登录态。重新登录视为换了一个会话：上次运行结果全部作废，
/// 否则页面会拿旧账号的余额冒充新账号的。
pub fn upsert_session(
    pool: &DbPool,
    platform_id: &str,
    platform: &str,
    identity: &str,
    label: &str,
    cookies_enc: &str,
    cookie_names: &str,
) -> AppResult<String> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let ts = now();
    if let Some(existing) = conn
        .query_row(
            "SELECT id FROM checkin_accounts WHERE platform_id = ?1",
            rusqlite::params![platform_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        conn.execute(
            "UPDATE checkin_accounts SET platform = ?2, identity = ?3, label = ?4, \
             cookies_enc = ?5, cookie_names = ?6, status = 'new', last_message = '', \
             last_run_at = NULL, last_run_day = NULL, last_ok_day = NULL, balance = NULL, \
             daily_amount = NULL, daily_done = 0, updated_at = ?7 WHERE id = ?1",
            rusqlite::params![existing, platform, identity, label, cookies_enc, cookie_names, ts],
        )?;
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO checkin_accounts (id, platform_id, platform, identity, label, cookies_enc, \
         cookie_names, status, last_message, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'new', '', ?8, ?8)",
        rusqlite::params![id, platform_id, platform, identity, label, cookies_enc, cookie_names, ts],
    )?;
    Ok(id)
}

pub fn update_run(pool: &DbPool, id: &str, update: &RunUpdate<'_>) -> AppResult<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE checkin_accounts SET status = ?2, last_message = ?3, last_run_at = ?4, \
         last_run_day = ?5, last_ok_day = COALESCE(?6, last_ok_day), \
         balance = COALESCE(?7, balance), daily_amount = COALESCE(?8, daily_amount), \
         daily_done = COALESCE(?9, daily_done), updated_at = ?4 WHERE id = ?1",
        rusqlite::params![
            id,
            update.status,
            update.message,
            update.run_at,
            update.run_day,
            update.ok_day,
            update.balance,
            update.daily_amount,
            update.daily_done.map(|done| done as i64),
        ],
    )?;
    Ok(())
}

pub fn delete_by_platform(pool: &DbPool, platform_id: &str) -> AppResult<bool> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let removed = conn.execute(
        "DELETE FROM checkin_accounts WHERE platform_id = ?1",
        rusqlite::params![platform_id],
    )?;
    Ok(removed > 0)
}
