use crate::error::AppResult;
use crate::models::api_key::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<ApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare("SELECT id, name, key, proxy_id, created_at, last_used FROM api_keys ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| Ok(ApiKey {
        id: row.get(0)?, name: row.get(1)?, key: row.get(2)?,
        proxy_id: row.get(3)?, created_at: row.get(4)?, last_used: row.get(5)?,
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_by_key(pool: &DbPool, key: &str) -> AppResult<Option<ApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row(
        "SELECT id, name, key, proxy_id, created_at, last_used FROM api_keys WHERE key=?1",
        [key],
        |row| Ok(ApiKey {
            id: row.get(0)?, name: row.get(1)?, key: row.get(2)?,
            proxy_id: row.get(3)?, created_at: row.get(4)?, last_used: row.get(5)?,
        }),
    );
    match result {
        Ok(api_key) => Ok(Some(api_key)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(crate::error::AppError::Internal(e.to_string())),
    }
}

/// Validate an API key against a specific proxy_id.
/// Returns true if the key is valid for the given proxy (or is a global key).
pub fn validate(pool: &DbPool, key: &str, proxy_id: &str) -> AppResult<bool> {
    let api_key = get_by_key(pool, key)?;
    match api_key {
        Some(ak) => Ok(ak.proxy_id.is_none() || ak.proxy_id.as_deref() == Some(proxy_id)),
        None => Ok(false),
    }
}

/// Validate an API key against any proxy (global key matches all).
/// Returns true if the key exists and is a global key.
pub fn validate_any(pool: &DbPool, key: &str) -> AppResult<bool> {
    let api_key = get_by_key(pool, key)?;
    Ok(api_key.is_some())
}

pub fn create(pool: &DbPool, req: &CreateApiKeyRequest) -> AppResult<ApiKey> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let api_key = ApiKey {
        id: id.clone(), name: req.name.clone(), key: req.key.clone(),
        proxy_id: req.proxy_id.clone(), created_at: now, last_used: None,
    };
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO api_keys (id, name, key, proxy_id, created_at, last_used) VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![api_key.id, api_key.name, api_key.key, api_key.proxy_id, api_key.created_at, api_key.last_used],
    )?;
    Ok(api_key)
}

pub fn update_last_used(pool: &DbPool, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("UPDATE api_keys SET last_used=?1 WHERE id=?2", rusqlite::params![now, id])?;
    Ok(())
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM api_keys WHERE id=?1", [id])?;
    Ok(())
}
