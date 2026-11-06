pub mod schema;
pub mod alert;
pub mod backup;
pub mod platform;
pub mod credential;
pub mod model;
pub mod proxy;
pub mod route;
pub mod stats;
pub mod api_key;
pub mod credit;
pub mod auth;
pub mod config_transfer;
pub mod client_config;
pub mod checkin;
pub mod checkin_task;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;
use crate::error::AppError;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn init_pool(db_path: &Path) -> Result<DbPool, AppError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| {
            // WAL 下 synchronous=NORMAL 仍是崩溃安全（只丢最后一次提交，不损坏库），
            // 但把每条 INSERT 的 fsync 降下来——StatsWriter 批量写依赖这一点。
            // busy_timeout 显式设置：并发写同一把 SQLite 写锁时等 5s 再报错，
            // 而不是立刻返回 database is locked（r2d2 默认连接超时 30s）。
            // wal_autocheckpoint 放宽到 4000 页，减少高写入下频繁 checkpoint 造成的停顿。
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA wal_autocheckpoint=4000;
                 PRAGMA cache_size=-16000;
                 PRAGMA temp_store=MEMORY;",
            )?;
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(8)
        // 池被长只读报表占满时快速失败，而不是让热路径排 30 秒
        .connection_timeout(std::time::Duration::from_secs(5))
        .build(manager)
        .map_err(|e| AppError::Internal(format!("Failed to create connection pool: {}", e)))?;
    
    // Run migrations on one connection
    let conn = pool.get().map_err(|e| AppError::Internal(format!("Failed to get connection: {}", e)))?;
    schema::run_migrations(&conn)?;
    
    Ok(pool)
}

/// Get a connection from the pool
pub fn get_conn(pool: &DbPool) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, AppError> {
    pool.get().map_err(|e| AppError::Internal(format!("Failed to get DB connection: {}", e)))
}

/// 桌面端托盘菜单的「模型名 + token 数」。
/// 刻意用一条裸连接而不是 `init_pool`：后者会建 8 连接池并跑一遍迁移，
/// 放在 UI 线程上就是启动卡顿的直接来源。这里只读、只开一个连接。
pub fn tray_menu_entries(db_path: &Path) -> Result<Vec<(String, String)>, AppError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| AppError::Internal(format!("Failed to open database: {}", e)))?;
    let mut stmt = conn
        .prepare("SELECT id, name FROM proxies ORDER BY name")
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, name) = row;
        let total_tokens: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(COALESCE(token_input,0) + COALESCE(token_output,0)), 0) \
                 FROM request_stats WHERE proxy_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let token_str = if total_tokens >= 1_000_000 {
            format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
        } else if total_tokens >= 1_000 {
            format!("{:.1}K", total_tokens as f64 / 1_000.0)
        } else {
            total_tokens.to_string()
        };
        out.push((id, format!("{}  Tokens: {}", name, token_str)));
    }
    Ok(out)
}
