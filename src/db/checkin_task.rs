//! 定时领取任务的持久化。一行 = 一个「几点跑、跑哪些账号」的日程。
//!
//! 任务只做领取；账号即平台行，所以 `platform_ids` 存的是平台记录 id 的 JSON 数组。

use rusqlite::OptionalExtension;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct CheckinTaskRow {
    pub id: String,
    pub name: String,
    /// `'HH:MM'`，本地时间
    pub time: String,
    pub platform_ids: Vec<String>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    /// 本地日期 `YYYY-MM-DD`：任务级「今天跑过没有」的记账
    pub last_run_day: Option<String>,
    pub last_message: String,
    pub created_at: String,
    pub updated_at: String,
}

const COLS: &str = "id, name, time, platform_ids, enabled, last_run_at, last_run_day, \
                    last_message, created_at, updated_at";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<CheckinTaskRow> {
    let raw: String = row.get(3)?;
    let ids = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
    Ok(CheckinTaskRow {
        id: row.get(0)?,
        name: row.get(1)?,
        time: row.get(2)?,
        platform_ids: ids,
        enabled: row.get::<_, i64>(4)? != 0,
        last_run_at: row.get(5)?,
        last_run_day: row.get(6)?,
        last_message: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

fn ids_json(ids: &[String]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string())
}

pub fn list(pool: &DbPool) -> AppResult<Vec<CheckinTaskRow>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM checkin_tasks ORDER BY time, created_at"))?;
    let rows = stmt.query_map([], map_row)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Option<CheckinTaskRow>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM checkin_tasks WHERE id = ?1"),
            rusqlite::params![id],
            map_row,
        )
        .optional()?)
}

pub fn create(
    pool: &DbPool,
    name: &str,
    time: &str,
    platform_ids: &[String],
    enabled: bool,
) -> AppResult<String> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO checkin_tasks (id, name, time, platform_ids, enabled, last_message, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?6, '', ?5, ?5)",
        rusqlite::params![id, name, time, ids_json(platform_ids), ts, enabled as i64],
    )?;
    Ok(id)
}

/// 整体覆盖（名称、点位、账号清单、开关）。改点位不清运行记录——
/// 「今天跑过」按日期记账，把 00:05 改成 23:50 不该让当天再跑一遍。
pub fn update(
    pool: &DbPool,
    id: &str,
    name: &str,
    time: &str,
    platform_ids: &[String],
    enabled: bool,
) -> AppResult<bool> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let changed = conn.execute(
        "UPDATE checkin_tasks SET name = ?2, time = ?3, platform_ids = ?4, enabled = ?6, updated_at = ?5 WHERE id = ?1",
        rusqlite::params![id, name, time, ids_json(platform_ids), now(), enabled as i64],
    )?;
    Ok(changed > 0)
}

pub fn update_run(pool: &DbPool, id: &str, run_at: &str, run_day: &str, message: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE checkin_tasks SET last_run_at = ?2, last_run_day = ?3, last_message = ?4, updated_at = ?2 \
         WHERE id = ?1",
        rusqlite::params![id, run_at, run_day, message],
    )?;
    Ok(())
}

pub fn remove(pool: &DbPool, id: &str) -> AppResult<bool> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let removed = conn.execute("DELETE FROM checkin_tasks WHERE id = ?1", rusqlite::params![id])?;
    Ok(removed > 0)
}

/// 删平台后从所有任务里剔除这个账号；任务可能因此变成空清单，由界面提示用户处理。
pub fn drop_platform(pool: &DbPool, platform_id: &str) -> AppResult<usize> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let ts = now();
    let mut stmt = conn.prepare("SELECT id, platform_ids FROM checkin_tasks")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .filter_map(|item| item.ok())
        .collect::<Vec<_>>();
    let mut touched = 0;
    for (id, raw) in rows {
        let ids = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
        let kept: Vec<String> = ids.iter().filter(|item| *item != platform_id).cloned().collect();
        if kept.len() != ids.len() {
            conn.execute(
                "UPDATE checkin_tasks SET platform_ids = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id, ids_json(&kept), ts],
            )?;
            touched += 1;
        }
    }
    Ok(touched)
}
