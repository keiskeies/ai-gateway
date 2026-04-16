use crate::error::AppResult;
use crate::models::proxy::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<Proxy>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare("SELECT id, name, listen_port, protocols, auth_token, status, created_at FROM proxies ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, listen_port: row.get(2)?,
        protocols: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        auth_token: row.get(4)?,
        status: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(ProxyStatus::Stopped),
        created_at: row.get(6)?,
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Proxy> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row("SELECT id, name, listen_port, protocols, auth_token, status, created_at FROM proxies WHERE id=?1", [id], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, listen_port: row.get(2)?,
        protocols: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        auth_token: row.get(4)?,
        status: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(ProxyStatus::Stopped),
        created_at: row.get(6)?,
    })).map_err(Into::into)
}

pub fn get_by_port(pool: &DbPool, port: u16) -> AppResult<Option<Proxy>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row("SELECT id, name, listen_port, protocols, auth_token, status, created_at FROM proxies WHERE listen_port=?1", [port], |row| Ok(Proxy {
        id: row.get(0)?, name: row.get(1)?, listen_port: row.get(2)?,
        protocols: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        auth_token: row.get(4)?,
        status: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(ProxyStatus::Stopped),
        created_at: row.get(6)?,
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
    let proxy = Proxy { id: id.clone(), name: req.name.clone(), listen_port: req.listen_port,
        protocols: req.protocols.clone(), auth_token: req.auth_token.clone(),
        status: ProxyStatus::Stopped, created_at: now };
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("INSERT INTO proxies (id, name, listen_port, protocols, auth_token, status, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![proxy.id, proxy.name, proxy.listen_port, serde_json::to_string(&proxy.protocols)?,
            proxy.auth_token, serde_json::to_string(&proxy.status)?, proxy.created_at])?;
    Ok(proxy)
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateProxyRequest) -> AppResult<Proxy> {
    let mut proxy = get(pool, id)?;
    if let Some(n) = &req.name { proxy.name = n.clone(); }
    if let Some(p) = req.listen_port { proxy.listen_port = p; }
    if let Some(p) = &req.protocols { proxy.protocols = p.clone(); }
    if let Some(t) = &req.auth_token { proxy.auth_token = Some(t.clone()); }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("UPDATE proxies SET name=?2, listen_port=?3, protocols=?4, auth_token=?5 WHERE id=?1",
        rusqlite::params![id, proxy.name, proxy.listen_port, serde_json::to_string(&proxy.protocols)?, proxy.auth_token])?;
    Ok(proxy)
}

pub fn update_status(pool: &DbPool, id: &str, status: &ProxyStatus) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("UPDATE proxies SET status=?2 WHERE id=?1", rusqlite::params![id, serde_json::to_string(status)?])?;
    Ok(())
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM proxies WHERE id=?1", [id])?;
    Ok(())
}
