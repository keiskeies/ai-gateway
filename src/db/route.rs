use crate::error::AppResult;
use crate::models::route::*;
use crate::db::DbPool;

/// Get the route for a proxy (1:1 relationship)
pub fn get_by_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<Option<Route>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row(
        "SELECT id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens FROM routes WHERE proxy_id=?1",
        [proxy_id],
        |row| Ok(Route {
            id: row.get(0)?, proxy_id: row.get(1)?,
            lb_strategy: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(LBStrategy::RoundRobin),
            retry_policy: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            fallback: row.get(4)?,
            default_max_tokens: row.get(5)?,
            backends: vec![],
        }),
    );
    match result {
        Ok(mut route) => {
            drop(conn);
            route.backends = list_backends_by_route(pool, &route.id)?;
            Ok(Some(route))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Route> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut route = conn.query_row(
        "SELECT id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens FROM routes WHERE id=?1",
        [id],
        |row| Ok(Route {
            id: row.get(0)?, proxy_id: row.get(1)?,
            lb_strategy: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(LBStrategy::RoundRobin),
            retry_policy: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            fallback: row.get(4)?,
            default_max_tokens: row.get(5)?,
            backends: vec![],
        }),
    )?;
    drop(conn);
    route.backends = list_backends_by_route(pool, id)?;
    Ok(route)
}

/// List routes by proxy (returns at most 1 route per proxy in the new model)
pub fn list_by_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<Vec<Route>> {
    match get_by_proxy(pool, proxy_id)? {
        Some(route) => Ok(vec![route]),
        None => Ok(vec![]),
    }
}

pub fn create(pool: &DbPool, proxy_id: &str, req: &CreateRouteRequest) -> AppResult<Route> {
    let route_id = uuid::Uuid::new_v4().to_string();
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO routes (id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens) VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![route_id, proxy_id, serde_json::to_string(&req.lb_strategy)?,
            serde_json::to_string(&req.retry_policy)?, req.fallback, req.default_max_tokens],
    )?;
    drop(conn);

    let mut backends = Vec::new();
    for breq in &req.backends {
        let bid = uuid::Uuid::new_v4().to_string();
        let b = Backend {
            id: bid.clone(), route_id: route_id.clone(), platform_id: breq.platform_id.clone(),
            model_id: breq.model_id.clone(), weight: breq.weight, priority: breq.priority,
            max_concurrent: breq.max_concurrent, capabilities: breq.capabilities.clone(), status: BackendStatus::Active,
            param_overrides: breq.param_overrides.clone(), unknown_params: breq.unknown_params.clone(),
        };
        let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO backends (id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![b.id, b.route_id, b.platform_id, b.model_id, b.weight, b.priority, b.max_concurrent, serde_json::to_string(&b.capabilities)?, serde_json::to_string(&b.status)?, serde_json::to_string(&b.param_overrides)?, serde_json::to_string(&b.unknown_params)?],
        )?;
        backends.push(b);
    }
    Ok(Route {
        id: route_id, proxy_id: proxy_id.to_string(),
        lb_strategy: req.lb_strategy.clone(), retry_policy: req.retry_policy.clone(),
        fallback: req.fallback.clone(), backends, default_max_tokens: req.default_max_tokens,
    })
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateRouteRequest) -> AppResult<Route> {
    let mut route = get(pool, id)?;
    if let Some(s) = &req.lb_strategy { route.lb_strategy = s.clone(); }
    if let Some(r) = &req.retry_policy { route.retry_policy = r.clone(); }
    if let Some(f) = &req.fallback { route.fallback = Some(f.clone()); }
    if let Some(m) = req.default_max_tokens { route.default_max_tokens = Some(m); }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE routes SET lb_strategy=?2, retry_policy=?3, fallback=?4, default_max_tokens=?5 WHERE id=?1",
        rusqlite::params![id, serde_json::to_string(&route.lb_strategy)?,
            serde_json::to_string(&route.retry_policy)?, route.fallback, route.default_max_tokens],
    )?;
    Ok(route)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM routes WHERE id=?1", [id])?;
    Ok(())
}

// Backend operations
pub fn list_backends_by_route(pool: &DbPool, route_id: &str) -> AppResult<Vec<Backend>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params FROM backends WHERE route_id=?1 ORDER BY priority, weight",
    )?;
    let rows = stmt.query_map([route_id], |row| Ok(Backend {
        id: row.get(0)?, route_id: row.get(1)?, platform_id: row.get(2)?, model_id: row.get(3)?,
        weight: row.get(4)?, priority: row.get(5)?, max_concurrent: row.get(6)?,
        capabilities: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        status: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or(BackendStatus::Active),
        param_overrides: row.get::<_, Option<String>>(9)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_param_overrides),
        unknown_params: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn add_backend(pool: &DbPool, route_id: &str, req: &CreateBackendRequest) -> AppResult<Backend> {
    let id = uuid::Uuid::new_v4().to_string();
    let b = Backend {
        id: id.clone(), route_id: route_id.to_string(), platform_id: req.platform_id.clone(),
        model_id: req.model_id.clone(), weight: req.weight, priority: req.priority,
        max_concurrent: req.max_concurrent, capabilities: req.capabilities.clone(), status: BackendStatus::Active,
        param_overrides: req.param_overrides.clone(), unknown_params: req.unknown_params.clone(),
    };
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO backends (id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        rusqlite::params![b.id, b.route_id, b.platform_id, b.model_id, b.weight, b.priority, b.max_concurrent, serde_json::to_string(&b.capabilities)?, serde_json::to_string(&b.status)?, serde_json::to_string(&b.param_overrides)?, serde_json::to_string(&b.unknown_params)?],
    )?;
    Ok(b)
}

pub fn update_backend(pool: &DbPool, id: &str, req: &UpdateBackendRequest) -> AppResult<Backend> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut b = conn.query_row(
        "SELECT id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params FROM backends WHERE id=?1",
        [id],
        |row| Ok(Backend {
            id: row.get(0)?, route_id: row.get(1)?, platform_id: row.get(2)?, model_id: row.get(3)?,
            weight: row.get(4)?, priority: row.get(5)?, max_concurrent: row.get(6)?,
            capabilities: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
            status: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or(BackendStatus::Active),
            param_overrides: row.get::<_, Option<String>>(9)?
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(default_param_overrides),
            unknown_params: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
        }),
    )?;
    if let Some(w) = req.weight { b.weight = w; }
    if let Some(p) = req.priority { b.priority = p; }
    if let Some(m) = req.max_concurrent { b.max_concurrent = Some(m); }
    if let Some(c) = &req.capabilities { b.capabilities = c.clone(); }
    if let Some(s) = &req.status { b.status = s.clone(); }
    if let Some(po) = &req.param_overrides { b.param_overrides = po.clone(); }
    if let Some(up) = &req.unknown_params { b.unknown_params = up.clone(); }
    conn.execute(
        "UPDATE backends SET weight=?2, priority=?3, max_concurrent=?4, capabilities=?5, status=?6, param_overrides=?7, unknown_params=?8 WHERE id=?1",
        rusqlite::params![id, b.weight, b.priority, b.max_concurrent, serde_json::to_string(&b.capabilities)?, serde_json::to_string(&b.status)?, serde_json::to_string(&b.param_overrides)?, serde_json::to_string(&b.unknown_params)?],
    )?;
    Ok(b)
}

pub fn delete_backend(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM backends WHERE id=?1", [id])?;
    Ok(())
}
