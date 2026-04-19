pub mod schema;
pub mod platform;
pub mod model;
pub mod proxy;
pub mod route;
pub mod stats;
pub mod api_key;

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
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(8)
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
