use crate::error::AppResult;
use crate::models::proxy::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<Proxy>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare("SELECT id, name, created_at FROM proxies ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)?, capabilities: vec![],
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Proxy> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row("SELECT id, name, created_at FROM proxies WHERE id=?1", [id], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)?, capabilities: vec![],
    })).map_err(Into::into)
}

/// Find a proxy by its name (the virtual model name)
pub fn get_by_name(pool: &DbPool, name: &str) -> AppResult<Option<Proxy>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row("SELECT id, name, created_at FROM proxies WHERE name=?1", [name], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)?, capabilities: vec![],
    }));
    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn create(pool: &DbPool, req: &CreateProxyRequest) -> AppResult<Proxy> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let proxy = Proxy { id: id.clone(), name: req.name.clone(), created_at: now, capabilities: vec![] };
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("INSERT INTO proxies (id, name, created_at) VALUES (?1,?2,?3)",
        rusqlite::params![proxy.id, proxy.name, proxy.created_at])?;
    Ok(proxy)
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateProxyRequest) -> AppResult<Proxy> {
    let mut proxy = get(pool, id)?;
    if let Some(n) = &req.name { proxy.name = n.clone(); }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("UPDATE proxies SET name=?2 WHERE id=?1", rusqlite::params![id, proxy.name])?;
    Ok(proxy)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM proxies WHERE id=?1", [id])?;
    Ok(())
}
