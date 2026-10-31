use crate::error::AppResult;
use crate::models::route::*;
use crate::db::DbPool;

// 后端读取列：末尾是价格/积分字段，来自按 (catalog_key, model_id) 的共享目录。
// 同一上游 URL 的账号共享 models 行，所以这里必须经 platforms.catalog_key 关联，
// 不能再按 m.platform_id = b.platform_id。
const BACKEND_COLS: &str = "b.id, b.route_id, b.platform_id, b.model_id, b.weight, b.priority, \
    b.max_concurrent, b.capabilities, b.status, b.param_overrides, b.unknown_params, \
    b.reasoning_style, b.reasoning_echo, m.input_price, m.output_price, m.credit_price, \
    m.cache_read_price, m.cache_write_price, m.peak_pricing, m.credit_multiplier";
const BACKEND_FROM: &str = "FROM backends b \
    LEFT JOIN platforms p ON p.id = b.platform_id \
    LEFT JOIN models m ON m.catalog_key = p.catalog_key AND m.model_id = b.model_id";

fn map_backend(row: &rusqlite::Row) -> rusqlite::Result<Backend> {
    Ok(Backend {
        id: row.get(0)?, route_id: row.get(1)?, platform_id: row.get(2)?, model_id: row.get(3)?,
        weight: row.get(4)?, priority: row.get(5)?, max_concurrent: row.get(6)?,
        capabilities: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        status: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or(BackendStatus::Active),
        param_overrides: row.get::<_, Option<String>>(9)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_param_overrides),
        unknown_params: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
        reasoning_style: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default(),
        reasoning_echo: serde_json::from_str(&row.get::<_, String>(12)?).unwrap_or_default(),
        input_price: row.get(13)?,
        output_price: row.get(14)?,
        credit_price: row.get(15)?,
        cache_read_price: row.get(16)?,
        cache_write_price: row.get(17)?,
        peak_pricing: row.get::<_, Option<String>>(18)?
            .and_then(|raw| serde_json::from_str(&raw).ok()),
        credit_multiplier: row.get(19)?,
    })
}

fn backend_by_id(pool: &DbPool, id: &str) -> AppResult<Backend> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row(
        &format!("SELECT {} {} WHERE b.id=?1", BACKEND_COLS, BACKEND_FROM),
        [id],
        map_backend,
    )
    .map_err(Into::into)
}

/// Get the route for a proxy (1:1 relationship)
pub fn get_by_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<Option<Route>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row(
        "SELECT id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens, request_nonce FROM routes WHERE proxy_id=?1",
        [proxy_id],
        |row| Ok(Route {
            id: row.get(0)?, proxy_id: row.get(1)?,
            lb_strategy: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(LBStrategy::RoundRobin),
            retry_policy: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            fallback: row.get(4)?,
            default_max_tokens: row.get(5)?,
            request_nonce: row.get::<_, i64>(6)? != 0,
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
        "SELECT id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens, request_nonce FROM routes WHERE id=?1",
        [id],
        |row| Ok(Route {
            id: row.get(0)?, proxy_id: row.get(1)?,
            lb_strategy: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(LBStrategy::RoundRobin),
            retry_policy: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            fallback: row.get(4)?,
            default_max_tokens: row.get(5)?,
            request_nonce: row.get::<_, i64>(6)? != 0,
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

/// 全部路由 + 后端：两条 SQL，替代前端按虚拟模型逐个拉取的 N+1
pub fn list_all(pool: &DbPool) -> AppResult<Vec<Route>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    let mut routes: Vec<Route> = {
        let mut stmt = conn.prepare(
            "SELECT id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens, request_nonce FROM routes",
        )?;
        let rows = stmt.query_map([], |row| Ok(Route {
            id: row.get(0)?, proxy_id: row.get(1)?,
            lb_strategy: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(LBStrategy::RoundRobin),
            retry_policy: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            fallback: row.get(4)?,
            default_max_tokens: row.get(5)?,
            request_nonce: row.get::<_, i64>(6)? != 0,
            backends: vec![],
        }))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut grouped: std::collections::HashMap<String, Vec<Backend>> = std::collections::HashMap::new();
    {
        // rowid 兜底：priority/weight 全等时（新建路由的默认值）没有第三列就是未定义顺序，
        // list_all 全表扫和 list_backends_by_route 带 WHERE 会给出不同次序，主干列表会自己重排。
        let sql = format!("SELECT {} {} ORDER BY b.priority, b.weight, b.rowid", BACKEND_COLS, BACKEND_FROM);
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_backend)?;
        for backend in rows {
            let backend = backend?;
            grouped.entry(backend.route_id.clone()).or_default().push(backend);
        }
    }

    for route in routes.iter_mut() {
        route.backends = grouped.remove(&route.id).unwrap_or_default();
    }
    Ok(routes)
}

pub fn create(pool: &DbPool, proxy_id: &str, req: &CreateRouteRequest) -> AppResult<Route> {
    let route_id = uuid::Uuid::new_v4().to_string();
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO routes (id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens, request_nonce) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![route_id, proxy_id, serde_json::to_string(&req.lb_strategy)?,
            serde_json::to_string(&req.retry_policy)?, req.fallback, req.default_max_tokens, req.request_nonce],
    )?;
    drop(conn);

    for breq in &req.backends {
        let bid = uuid::Uuid::new_v4().to_string();
        let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO backends (id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params, reasoning_style, reasoning_echo) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![bid, route_id, breq.platform_id, breq.model_id, breq.weight, breq.priority, breq.max_concurrent, serde_json::to_string(&breq.capabilities)?, serde_json::to_string(&BackendStatus::Active)?, serde_json::to_string(&breq.param_overrides)?, serde_json::to_string(&breq.unknown_params)?, serde_json::to_string(&breq.reasoning_style)?, serde_json::to_string(&breq.reasoning_echo)?],
        )?;
    }
    // 回读而不是用手写的结构体：单价只在带 LEFT JOIN models 的查询里才拿得到
    let backends = list_backends_by_route(pool, &route_id)?;
    Ok(Route {
        id: route_id, proxy_id: proxy_id.to_string(),
        lb_strategy: req.lb_strategy.clone(), retry_policy: req.retry_policy.clone(),
        fallback: req.fallback.clone(), backends, default_max_tokens: req.default_max_tokens,
        request_nonce: req.request_nonce,
    })
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateRouteRequest) -> AppResult<Route> {
    let mut route = get(pool, id)?;
    if let Some(s) = &req.lb_strategy { route.lb_strategy = s.clone(); }
    if let Some(r) = &req.retry_policy { route.retry_policy = r.clone(); }
    if let Some(f) = &req.fallback { route.fallback = Some(f.clone()); }
    if let Some(m) = req.default_max_tokens { route.default_max_tokens = Some(m); }
    if let Some(n) = req.request_nonce { route.request_nonce = n; }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE routes SET lb_strategy=?2, retry_policy=?3, fallback=?4, default_max_tokens=?5, request_nonce=?6 WHERE id=?1",
        rusqlite::params![id, serde_json::to_string(&route.lb_strategy)?,
            serde_json::to_string(&route.retry_policy)?, route.fallback, route.default_max_tokens, route.request_nonce],
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
    let sql = format!(
        "SELECT {} {} WHERE b.route_id=?1 ORDER BY b.priority, b.weight, b.rowid",
        BACKEND_COLS, BACKEND_FROM
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([route_id], map_backend)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn add_backend(pool: &DbPool, route_id: &str, req: &CreateBackendRequest) -> AppResult<Backend> {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO backends (id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status, param_overrides, unknown_params, reasoning_style, reasoning_echo) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        rusqlite::params![id, route_id, req.platform_id, req.model_id, req.weight, req.priority, req.max_concurrent, serde_json::to_string(&req.capabilities)?, serde_json::to_string(&BackendStatus::Active)?, serde_json::to_string(&req.param_overrides)?, serde_json::to_string(&req.unknown_params)?, serde_json::to_string(&req.reasoning_style)?, serde_json::to_string(&req.reasoning_echo)?],
    )?;
    drop(conn);
    // 回读：单价只在带 LEFT JOIN 的查询里才有，手写结构体给不出真实价格
    backend_by_id(pool, &id)
}

pub fn update_backend(pool: &DbPool, id: &str, req: &UpdateBackendRequest) -> AppResult<Backend> {
    let mut b = backend_by_id(pool, id)?;
    if let Some(w) = req.weight { b.weight = w; }
    if let Some(p) = req.priority { b.priority = p; }
    if let Some(m) = req.max_concurrent { b.max_concurrent = Some(m); }
    if let Some(c) = &req.capabilities { b.capabilities = c.clone(); }
    if let Some(s) = &req.status { b.status = s.clone(); }
    if let Some(po) = &req.param_overrides { b.param_overrides = po.clone(); }
    if let Some(up) = &req.unknown_params { b.unknown_params = up.clone(); }
    if let Some(rs) = &req.reasoning_style { b.reasoning_style = rs.clone(); }
    if let Some(re) = &req.reasoning_echo { b.reasoning_echo = re.clone(); }
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE backends SET weight=?2, priority=?3, max_concurrent=?4, capabilities=?5, status=?6, param_overrides=?7, unknown_params=?8, reasoning_style=?9, reasoning_echo=?10 WHERE id=?1",
        rusqlite::params![id, b.weight, b.priority, b.max_concurrent, serde_json::to_string(&b.capabilities)?, serde_json::to_string(&b.status)?, serde_json::to_string(&b.param_overrides)?, serde_json::to_string(&b.unknown_params)?, serde_json::to_string(&b.reasoning_style)?, serde_json::to_string(&b.reasoning_echo)?],
    )?;
    Ok(b)
}

/// 按给定次序把 priority 重写成 0..n。轮询取的是后端切片的下标，
/// 所以这个次序就是实际的轮询次序；优先级策略下它同时是故障转移次序。
pub fn set_order(pool: &DbPool, route_id: &str, ordered_ids: &[String]) -> AppResult<Vec<Backend>> {
    let mut conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    let existing: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM backends WHERE route_id=?1 ORDER BY rowid")?;
        let rows = stmt.query_map([route_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if existing.len() != ordered_ids.len() || existing.iter().collect::<std::collections::HashSet<_>>()
        != ordered_ids.iter().collect::<std::collections::HashSet<_>>()
    {
        return Err(crate::error::AppError::BadRequest(
            "顺序列表必须覆盖该路由的全部后端，且不能重复".to_string(),
        ));
    }

    let tx = conn.transaction()?;
    for (index, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE backends SET priority=?1 WHERE id=?2 AND route_id=?3",
            rusqlite::params![index as u32, id, route_id],
        )?;
    }
    tx.commit()?;

    list_backends_by_route(pool, route_id)
}

pub fn delete_backend(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM backends WHERE id=?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::model::{Capability, CreateModelRequest};
    use crate::models::platform::{CreatePlatformRequest, PlatformType};
    use crate::models::proxy::CreateProxyRequest;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("route-test-{}.db", uuid::Uuid::new_v4()));
        (crate::db::init_pool(&path).expect("init test db"), path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    /// 单价是 models 表的列，backend 行本身不存价：读取路径的 LEFT JOIN 必须按
    /// (platform_id, model_id) 把价格带出来，否则热路径算不出成本。
    #[test]
    fn backend_reads_prices_from_the_matching_model_row() {
        let (pool, path) = test_db();
        let platform = crate::db::platform::create(&pool, &CreatePlatformRequest {
            name: "plat".into(),
            platform_type: PlatformType::OpenAI,
            base_url: "http://127.0.0.1:1/v1".into(),
            api_key: "sk-test".into(),
            organization: None,
            custom_headers: serde_json::Value::Null,
            credit_budget: None,
            credit_window: None,
            credit_unit: None,
            adapter: None,
        }).expect("create platform");
        let proxy = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "virtual-model".into() })
            .expect("create proxy");

        // 先建后端：此刻 models 里还没有对应行，价格应为 None（而不是崩或假 0）
        let route = crate::db::route::create(&pool, &proxy.id, &CreateRouteRequest {
            lb_strategy: LBStrategy::RoundRobin,
            retry_policy: RetryPolicy::default(),
            fallback: None,
            default_max_tokens: None,
            request_nonce: false,
            backends: vec![CreateBackendRequest {
                platform_id: platform.id.clone(),
                model_id: "gpt-test".into(),
                weight: 1, priority: 0, max_concurrent: None, capabilities: vec![],
                param_overrides: serde_json::json!({}),
                unknown_params: UnknownParams::Auto,
                reasoning_style: ReasoningStyle::Auto,
                reasoning_echo: ReasoningEcho::default(),
            }],
        }).expect("create route");
        let backend_id = route.backends[0].id.clone();
        assert_eq!((route.backends[0].input_price, route.backends[0].output_price), (None, None));

        // 只填输入价：两列各自读得出来，但 price_pair 仍为 None（整单不估算）
        crate::db::model::create(&pool, &CreateModelRequest {
            platform_id: platform.id.clone(),
            model_id: "gpt-test".into(),
            display_name: "GPT Test".into(),
            max_tokens: 4096, context_window: 8192,
            input_price: Some(2.5), output_price: None,
            credit_price: None,
            credit_multiplier: None,
            cache_read_price: None,
            cache_write_price: None,
            peak_pricing: None,
            capabilities: vec![Capability::Reasoning],
        }).expect("create model (input only)");
        let b = backend_by_id(&pool, &backend_id).expect("read backend");
        assert_eq!((b.input_price, b.output_price), (Some(2.5), None));
        assert_eq!(b.price_pair(), None);

        // 补上输出价：list_backends_by_route 与 list_all 都应带出单价对
        let mut conn = pool.get().unwrap();
        conn.execute("UPDATE models SET output_price = 4.0 WHERE platform_id = ?1 AND model_id = ?2",
            rusqlite::params![platform.id, "gpt-test"]).unwrap();
        drop(conn);
        let listed = list_backends_by_route(&pool, &route.id).expect("list");
        assert_eq!(listed[0].price_pair(), Some((2.5, 4.0)));
        let all = list_all(&pool).expect("list all");
        assert_eq!(all[0].backends[0].price_pair(), Some((2.5, 4.0)));

        // 同 model_id 但属于别的平台的行不参与匹配（价格必须按平台隔离）
        let other = crate::db::platform::create(&pool, &CreatePlatformRequest {
            name: "plat-2".into(), platform_type: PlatformType::OpenAI,
            base_url: "http://127.0.0.1:2/v1".into(), api_key: "sk-2".into(),
            organization: None, custom_headers: serde_json::Value::Null,
            credit_budget: None, credit_window: None,
            credit_unit: None,
            adapter: None,
        }).expect("create second platform");
        let mut conn = pool.get().unwrap();
        conn.execute("INSERT INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status) VALUES ('m-other', ?1, 'gpt-test', 'Other', 4096, 8192, 9.9, 9.9, '[]', 'Active')",
            rusqlite::params![other.id]).unwrap();
        drop(conn);
        assert_eq!(backend_by_id(&pool, &backend_id).expect("recheck").price_pair(), Some((2.5, 4.0)));

        cleanup_db(&path);
    }
}
