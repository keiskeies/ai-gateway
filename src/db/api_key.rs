use sha2::{Digest, Sha256};

use crate::error::AppResult;
use crate::models::api_key::*;
use crate::db::DbPool;

/// 网关 Key 的校验摘要：sha256(明文) 的 hex。
/// 选 sha256 而不是像管理端口令那样用 Argon2：这是每个请求都要走的热路径，
/// 而 Key 本身是 32 字节随机数——离线暴破在计算上本来就不成立，慢哈希只会拖慢鉴权。
pub fn hash_key(plain: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plain.as_bytes());
    hex::encode(hasher.finalize())
}

/// 列表里用来"认人"的掩码：与平台密钥同一套规则（头尾各留 4 个字符）。
pub fn key_mask(plain: &str) -> String {
    crate::models::platform::mask_secret(plain)
}

const COLS: &str = "id, name, key_hash, key_mask, proxy_id, created_at, last_used, enabled, expires_at, quota_requests, quota_tokens, quota_cost, quota_window";

fn row_to_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredApiKey> {
    Ok(StoredApiKey {
        id: row.get(0)?,
        name: row.get(1)?,
        key_hash: row.get(2)?,
        key_mask: row.get(3)?,
        proxy_id: row.get(4)?,
        created_at: row.get(5)?,
        last_used: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        expires_at: row.get(8)?,
        quota_requests: row.get(9)?,
        quota_tokens: row.get(10)?,
        quota_cost: row.get(11)?,
        quota_window: row.get(12)?,
    })
}

pub fn list(pool: &DbPool) -> AppResult<Vec<StoredApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM api_keys ORDER BY created_at"))?;
    let rows = stmt.query_map([], row_to_key)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Option<StoredApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row(
        &format!("SELECT {COLS} FROM api_keys WHERE id=?1"),
        [id],
        row_to_key,
    );
    match result {
        Ok(api_key) => Ok(Some(api_key)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(crate::error::AppError::Internal(e.to_string())),
    }
}

/// 按明文密钥查行：库里没有明文，所以这里对"客户端传来的值"取同一个摘要再比。
pub fn get_by_key(pool: &DbPool, key: &str) -> AppResult<Option<StoredApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let result = conn.query_row(
        &format!("SELECT {COLS} FROM api_keys WHERE key_hash=?1"),
        [hash_key(key)],
        row_to_key,
    );
    match result {
        Ok(api_key) => Ok(Some(api_key)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(crate::error::AppError::Internal(e.to_string())),
    }
}

/// Validate an API key against a specific proxy_id.
/// Returns true if the key is enabled and valid for the given proxy (or is a global key).
pub fn validate(pool: &DbPool, key: &str, proxy_id: &str) -> AppResult<bool> {
    let api_key = get_by_key(pool, key)?;
    match api_key {
        Some(ak) => Ok(ak.enabled && (ak.proxy_id.is_none() || ak.proxy_id.as_deref() == Some(proxy_id))),
        None => Ok(false),
    }
}

/// Validate an API key against any proxy (global key matches all).
pub fn validate_any(pool: &DbPool, key: &str) -> AppResult<bool> {
    let api_key = get_by_key(pool, key)?;
    Ok(api_key.map(|ak| ak.enabled).unwrap_or(false))
}

/// 落库的是摘要与掩码，明文只活在这条语句返回之前的栈上。
/// 调用方（`api/api_key.rs`）自己留着那份明文，用于一次性 `secret` 回显。
pub fn create(pool: &DbPool, req: &CreateApiKeyRequest) -> AppResult<StoredApiKey> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let plain = req.key.clone().unwrap_or_default();
    let api_key = StoredApiKey {
        id,
        name: req.name.clone(),
        key_hash: hash_key(&plain),
        key_mask: key_mask(&plain),
        proxy_id: req.proxy_id.clone(),
        created_at: now,
        last_used: None,
        enabled: true,
        expires_at: req.expires_at.clone().filter(|value| !value.trim().is_empty()),
        quota_requests: req.quota_requests.filter(|value| *value > 0),
        quota_tokens: req.quota_tokens.filter(|value| *value > 0),
        quota_cost: req.quota_cost.filter(|value| *value > 0.0),
        quota_window: canonical_quota_window(req.quota_window.as_deref()).map(str::to_string),
    };
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO api_keys (id, name, key_hash, key_mask, proxy_id, created_at, last_used, enabled, expires_at, quota_requests, quota_tokens, quota_cost, quota_window) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        rusqlite::params![
            api_key.id,
            api_key.name,
            api_key.key_hash,
            api_key.key_mask,
            api_key.proxy_id,
            api_key.created_at,
            api_key.last_used,
            if api_key.enabled { 1 } else { 0 },
            api_key.expires_at,
            api_key.quota_requests,
            api_key.quota_tokens,
            api_key.quota_cost,
            api_key.quota_window,
        ],
    )?;
    Ok(api_key)
}

/// 三态列（Keep / Clear / Set）只能用拼 SET 子句的方式表达：
/// `COALESCE(?n, col)` 只能"没写就保持"，永远清不掉旧值——而"把 Key 改回全局"正是要清空 `proxy_id`。
pub fn update(pool: &DbPool, id: &str, req: &UpdateApiKeyRequest) -> AppResult<Option<StoredApiKey>> {
    let mut sets: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(name) = req.name.as_deref() {
        sets.push("name=?".to_string());
        args.push(Box::new(name.to_string()));
    }
    if let Some(enabled) = req.enabled {
        sets.push("enabled=?".to_string());
        args.push(Box::new(i64::from(enabled)));
    }
    match &req.proxy_id {
        NullableString::Keep => {}
        NullableString::Clear => { sets.push("proxy_id=NULL".to_string()); }
        NullableString::Set(value) => { sets.push("proxy_id=?".to_string()); args.push(Box::new(value.clone())); }
    }
    match &req.expires_at {
        NullableString::Keep => {}
        NullableString::Clear => { sets.push("expires_at=NULL".to_string()); }
        NullableString::Set(value) => { sets.push("expires_at=?".to_string()); args.push(Box::new(value.clone())); }
    }
    push_nullable_number(&mut sets, &mut args, "quota_requests", req.quota_requests);
    push_nullable_number(&mut sets, &mut args, "quota_tokens", req.quota_tokens);
    push_nullable_cost(&mut sets, &mut args, "quota_cost", req.quota_cost);
    match &req.quota_window {
        NullableString::Keep => {}
        NullableString::Clear => { sets.push("quota_window=NULL".to_string()); }
        NullableString::Set(value) => {
            sets.push("quota_window=?".to_string());
            args.push(Box::new(canonical_quota_window(Some(value)).map(str::to_string)));
        }
    }

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    if !sets.is_empty() {
        let sql = format!("UPDATE api_keys SET {} WHERE id=?", sets.join(", "));
        let id_param = id.to_string();
        let mut params: Vec<&dyn rusqlite::types::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        params.push(&id_param);
        conn.execute(&sql, params.as_slice())?;
    }
    let exists: Option<String> = conn
        .query_row("SELECT id FROM api_keys WHERE id=?1", [id], |row| row.get(0))
        .ok();
    if exists.is_none() {
        return Ok(None);
    }
    drop(conn);
    get(pool, id)
}

fn push_nullable_number(
    sets: &mut Vec<String>,
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    column: &str,
    value: crate::models::model::NullableNumber,
) {
    match value {
        crate::models::model::NullableNumber::Keep => {}
        crate::models::model::NullableNumber::Clear => sets.push(format!("{column}=NULL")),
        crate::models::model::NullableNumber::Set(v) => {
            // 配额只接受正数：0 与负数一律当作"清空"，避免设一个谁也过不了的配额
            let as_int = v.max(0.0) as i64;
            if as_int > 0 {
                sets.push(format!("{column}=?"));
                args.push(Box::new(as_int));
            } else {
                sets.push(format!("{column}=NULL"));
            }
        }
    }
}

/// 成本配额是金额，不能走 `push_nullable_number`：那条会把 12.50 截成 12。
/// 同样的"非正数即清空"规则，只是保留小数。
fn push_nullable_cost(
    sets: &mut Vec<String>,
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    column: &str,
    value: crate::models::model::NullableNumber,
) {
    match value {
        crate::models::model::NullableNumber::Keep => {}
        crate::models::model::NullableNumber::Clear => sets.push(format!("{column}=NULL")),
        crate::models::model::NullableNumber::Set(v) if v > 0.0 => {
            sets.push(format!("{column}=?"));
            args.push(Box::new(v));
        }
        crate::models::model::NullableNumber::Set(_) => sets.push(format!("{column}=NULL")),
    }
}

const USAGE_COLS: &str = "COUNT(*), \
        COALESCE(SUM(COALESCE(token_input,0) + COALESCE(token_output,0)), 0), \
        SUM(cost), COUNT(cost)";

/// 四条聚合列在两条语句里的起始下标不同（列表那条前面还有一列 Key id），所以带偏移取。
fn usage_at(row: &rusqlite::Row<'_>, base: usize) -> rusqlite::Result<KeyUsage> {
    Ok(KeyUsage {
        requests: row.get(base)?,
        tokens: row.get(base + 1)?,
        cost: row.get(base + 2)?,
        priced_requests: row.get(base + 3)?,
    })
}

/// 一把 Key 用掉多少请求、Token 与成本：配额判定与列表显示共用这一份口径，
/// 所以"界面上写着 42/100"和"第 101 次被拒"不会互相矛盾。
/// 统计行会被删（删除虚拟模型、清理统计），所以这是"现存统计里的用量"，不是不可变台账。
/// `window` 非空时只算窗口内（`created_at >= 窗口起点`），与统计页/告警同一套字符串比较口径。
/// 成本只统计已定价的行，`priced_requests` 就是那份覆盖率的分母说明。
pub fn usage_for_key(pool: &DbPool, id: &str, window: Option<&str>) -> AppResult<KeyUsage> {
    let since = window_start(window, chrono::Utc::now());
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let row = conn.query_row(
        &format!("SELECT {USAGE_COLS} FROM request_stats \
                  WHERE api_key_id=?1 AND (?2 IS NULL OR created_at>=?2)"),
        rusqlite::params![id, since],
        |row| usage_at(row, 0),
    );
    match row {
        Ok(usage) => Ok(usage),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(KeyUsage::default()),
        Err(e) => Err(crate::error::AppError::Internal(e.to_string())),
    }
}

/// 列表用：一次 GROUP BY 拿到所有 Key 的用量，避免逐行查（N+1）。
/// 每把 Key 按自己的 `quota_window` 取不同的时间下界，所以列表上的分子与闸门看到的分子一致。
pub fn usage_by_key(pool: &DbPool) -> AppResult<std::collections::HashMap<String, KeyUsage>> {
    let now = chrono::Utc::now();
    let (day, week, month) = (
        window_start(Some("day"), now),
        window_start(Some("week"), now),
        window_start(Some("month"), now),
    );
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT k.id, \
                COUNT(s.id), \
                COALESCE(SUM(COALESCE(s.token_input,0) + COALESCE(s.token_output,0)), 0), \
                SUM(s.cost), COUNT(s.cost) \
         FROM api_keys k \
         LEFT JOIN request_stats s ON s.api_key_id = k.id \
           AND (k.quota_window IS NULL \
                OR (k.quota_window='day'   AND s.created_at>=?1) \
                OR (k.quota_window='week'  AND s.created_at>=?2) \
                OR (k.quota_window='month' AND s.created_at>=?3)) \
         GROUP BY k.id",
    )?;
    let rows = stmt.query_map(rusqlite::params![day, week, month], |row| {
        Ok((row.get::<_, String>(0)?, usage_at(row, 1)?))
    })?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (id, usage) = row?;
        out.insert(id, usage);
    }
    Ok(out)
}

/// 轮换：只换密钥值，id / 名称 / 绑定虚拟模型 / enabled 全部保留，
/// 这样调用方改一处 Header 就能继续用，历史统计也仍挂在同一个 id 上。
pub fn rotate(pool: &DbPool, id: &str, new_key: &str) -> AppResult<Option<StoredApiKey>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let changed = conn.execute(
        "UPDATE api_keys SET key_hash=?1, key_mask=?2 WHERE id=?3",
        rusqlite::params![hash_key(new_key), key_mask(new_key), id],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    drop(conn);
    get(pool, id)
}

/// 删除虚拟模型时调用：绑定它的 Key 一并停用。
/// 这类 Key 其实已经无法通过鉴权（proxy_id 匹配不到任何路由），但 enabled 仍显示为真，
/// 停用让状态变得诚实；proxy_id 保留，调用方还能看出它原本属于哪个模型。
pub fn disable_for_proxy(pool: &DbPool, proxy_id: &str) -> AppResult<usize> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let disabled = conn.execute(
        "UPDATE api_keys SET enabled=0 WHERE proxy_id=?1 AND enabled=1",
        [proxy_id],
    )?;
    Ok(disabled)
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

fn id_placeholders(ids: &[String]) -> String {
    vec!["?"; ids.len()].join(",")
}

fn params<'a>(ids: &'a [String]) -> Vec<&'a dyn rusqlite::types::ToSql> {
    ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect()
}

/// 这批 id 里哪些在库里不存在。调用方要把它单独回给用户：
/// "点了 12 个、只改了 11 个"如果显示成"已启用 12 个"，就是在骗人。
fn missing_ids(conn: &rusqlite::Connection, ids: &[String]) -> AppResult<Vec<String>> {
    let sql = format!("SELECT id FROM api_keys WHERE id IN ({})", id_placeholders(ids));
    let mut stmt = conn.prepare(&sql)?;
    let found: Vec<String> = stmt
        .query_map(params(ids).as_slice(), |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    Ok(ids.iter().filter(|id| !found.iter().any(|hit| hit == *id)).cloned().collect())
}

/// 批量启用/停用：一条 UPDATE 覆盖整批，返回 (改动数, 不存在的 id)。
/// 只改 enabled——名称、绑定、过期与配额原样保留，与单行 PUT 的 `enabled` 同一语义。
pub fn batch_set_enabled(pool: &DbPool, ids: &[String], enabled: bool) -> AppResult<(usize, Vec<String>)> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let missing = missing_ids(&conn, ids)?;
    let sql = format!("UPDATE api_keys SET enabled=? WHERE id IN ({})", id_placeholders(ids));
    let mut args: Vec<&dyn rusqlite::types::ToSql> = vec![&enabled];
    args.extend(params(ids));
    let affected = conn.execute(&sql, args.as_slice())?;
    Ok((affected, missing))
}

/// 批量删除：历史请求仍留在 `request_stats`（`api_key_id` 没有外键），
/// 只是这些行不再对应任何 Key 名称。
pub fn batch_delete(pool: &DbPool, ids: &[String]) -> AppResult<(usize, Vec<String>)> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let missing = missing_ids(&conn, ids)?;
    let sql = format!("DELETE FROM api_keys WHERE id IN ({})", id_placeholders(ids));
    let affected = conn.execute(&sql, params(ids).as_slice())?;
    Ok((affected, missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::api_key::CreateApiKeyRequest;
    use crate::models::proxy::CreateProxyRequest;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("api-key-test-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::init_pool(&path).expect("init test db");
        (pool, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn make_key(pool: &DbPool, key: &str, proxy_id: Option<String>) {
        create(pool, &CreateApiKeyRequest {
            name: "k".to_string(),
            key: Some(key.to_string()),
            proxy_id,
            ..Default::default()
        }).expect("create key");
    }

    /// 明文只进不出：库里只有摘要与掩码，掩码拿去鉴权必然失败。
    #[test]
    fn plaintext_never_comes_back_out() {
        let (pool, path) = test_db();
        let plain = "sk-aigw-0123456789abcdef0123456789abcdef";
        let stored = create(&pool, &CreateApiKeyRequest {
            name: "hashed".to_string(),
            key: Some(plain.to_string()),
            proxy_id: None,
            ..Default::default()
        }).expect("create key");

        assert_eq!(stored.key_hash, hash_key(plain));
        assert_eq!(stored.key_mask, crate::models::platform::mask_secret(plain));
        assert_ne!(stored.key_mask, plain, "掩码不能就是原值");

        assert!(get_by_key(&pool, plain).unwrap().is_some());
        assert!(validate_any(&pool, plain).unwrap());
        // 从列表接口抄到的掩码不是凭据
        assert!(get_by_key(&pool, &stored.key_mask).unwrap().is_none());
        assert!(!validate_any(&pool, &stored.key_mask).unwrap());

        let conn = pool.get().unwrap();
        let dump: String = conn.query_row(
            "SELECT group_concat(key_hash || '|' || key_mask || '|' || name) FROM api_keys",
            [], |r| r.get(0),
        ).unwrap();
        assert!(!dump.contains(plain), "任何一列都不该含明文");

        // 摘要在 UNIQUE 上，重复明文建不出第二条
        let again = create(&pool, &CreateApiKeyRequest {
            name: "dup".to_string(),
            key: Some(plain.to_string()),
            proxy_id: None,
            ..Default::default()
        });
        assert!(again.is_err());

        cleanup_db(&path);
    }

    /// 轮换后旧值立刻失效，其余字段（含历史统计挂的 id）保持不变。
    #[test]
    fn rotation_revokes_the_previous_secret() {
        let (pool, path) = test_db();
        let first = "sk-aigw-first-first-first-first-0000";
        let second = "sk-aigw-second-second-second-1111";
        let created = create(&pool, &CreateApiKeyRequest {
            name: "rotate-me".to_string(),
            key: Some(first.to_string()),
            proxy_id: None,
            ..Default::default()
        }).expect("create key");

        let rotated = rotate(&pool, &created.id, second).unwrap().expect("rotated");
        assert_eq!(rotated.id, created.id);
        assert_eq!(rotated.name, created.name);
        assert_eq!(rotated.key_hash, hash_key(second));

        assert!(get_by_key(&pool, first).unwrap().is_none(), "旧明文必须立刻失效");
        assert!(validate_any(&pool, second).unwrap());
        assert!(rotate(&pool, "missing-id", second).unwrap().is_none());

        cleanup_db(&path);
    }

    /// 删除虚拟模型不能让绑定 Key 升级成全局凭据：绑定关系保留，Key 停用。
    #[test]
    fn deleting_a_proxy_keeps_binding_and_disables_key() {
        let (pool, path) = test_db();
        let proxy = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "bound-model".to_string() }).unwrap();
        let other = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "other-model".to_string() }).unwrap();
        make_key(&pool, "sk-bound-test", Some(proxy.id.clone()));
        make_key(&pool, "sk-global-test", None);

        assert_eq!(disable_for_proxy(&pool, &proxy.id).unwrap(), 1);
        crate::db::proxy::delete(&pool, &proxy.id).unwrap();

        let bound = get_by_key(&pool, "sk-bound-test").unwrap().expect("bound key row survives its model");
        assert_eq!(bound.proxy_id.as_deref(), Some(proxy.id.as_str()));
        assert!(!bound.enabled);
        assert!(!validate(&pool, "sk-bound-test", &other.id).unwrap());
        assert!(!validate_any(&pool, "sk-bound-test").unwrap());

        let global = get_by_key(&pool, "sk-global-test").unwrap().expect("global key");
        assert!(global.enabled);
        assert!(global.proxy_id.is_none());
        assert!(validate_any(&pool, "sk-global-test").unwrap());

        // 重复调用不再停用任何 Key
        assert_eq!(disable_for_proxy(&pool, &proxy.id).unwrap(), 0);
        cleanup_db(&path);
    }

    /// 过期 / 配额 / 改绑：三态更新要能设、能改、也能清空。
    #[test]
    fn expiry_quota_and_rebind_round_trip() {
        use crate::models::api_key::{NullableString, UpdateApiKeyRequest};
        use crate::models::model::NullableNumber;

        let (pool, path) = test_db();
        let proxy = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "quota-model".to_string() }).unwrap();
        let other = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "next-model".to_string() }).unwrap();

        let created = create(&pool, &CreateApiKeyRequest {
            name: "metered".to_string(),
            key: Some("sk-quota-test-quota-test-0001".to_string()),
            proxy_id: Some(proxy.id.clone()),
            expires_at: Some("2999-01-01T00:00:00Z".to_string()),
            quota_requests: Some(2),
            quota_tokens: Some(0),
            quota_cost: Some(-1.0),
            quota_window: Some("lifetime".to_string()),
        })
        .expect("create metered key");
        assert_eq!(created.quota_requests, Some(2));
        assert_eq!(created.expires_at.as_deref(), Some("2999-01-01T00:00:00Z"));
        assert!(!created.is_expired(chrono::Utc::now()));
        // 0 与负数不是"限制"，是写错了：一律当没设，免得设出一把谁也过不了的 Key
        assert_eq!(created.quota_tokens, None);
        assert_eq!(created.quota_cost, None);
        // "lifetime" 与非法写法都归成 NULL，库里只留三个可识别的窗口名
        assert_eq!(created.quota_window, None);

        // 配额的分母就是 request_stats 里这把 Key 的行数
        assert_eq!(usage_for_key(&pool, &created.id, None).unwrap(), KeyUsage::default());
        record_stat(&pool, &created.id, 3, 4, None);
        record_stat(&pool, &created.id, 100, 200, Some(0.5));
        let metered = KeyUsage { requests: 2, tokens: 307, cost: Some(0.5), priced_requests: 1 };
        assert_eq!(usage_for_key(&pool, &created.id, None).unwrap(), metered);
        assert_eq!(usage_by_key(&pool).unwrap().get(&created.id).copied(), Some(metered));

        // 改绑 + 清空到期 + 设 Token 配额 + 清掉请求配额，一次做完
        update(&pool, &created.id, &UpdateApiKeyRequest {
            name: None,
            enabled: None,
            proxy_id: NullableString::Set(other.id.clone()),
            expires_at: NullableString::Clear,
            quota_requests: NullableNumber::Clear,
            quota_tokens: NullableNumber::Set(1000.0),
            quota_cost: NullableNumber::Keep,
            quota_window: NullableString::Keep,
        })
        .unwrap()
        .expect("updated");

        let row = get(&pool, &created.id).unwrap().expect("still there");
        assert_eq!(row.proxy_id.as_deref(), Some(other.id.as_str()), "改绑生效");
        assert_eq!(row.expires_at, None, "null 清空生效");
        assert_eq!(row.quota_requests, None, "配额能清");
        assert_eq!(row.quota_tokens, Some(1000));
        assert_eq!(row.name, "metered");
        assert!(row.enabled, "没写的列不该被动到");
        assert_eq!(usage_for_key(&pool, &created.id, None).unwrap(), metered, "换绑不动用量");

        // 成本配额是小数：12.5 不能被截成 12
        update(&pool, &created.id, &UpdateApiKeyRequest {
            quota_cost: NullableNumber::Set(12.5),
            quota_window: NullableString::Set("MONTH".to_string()),
            ..Default::default()
        })
        .unwrap()
        .expect("cost quota set");
        let priced = get(&pool, &created.id).unwrap().expect("still there");
        assert_eq!(priced.quota_cost, Some(12.5), "金额配额要保留小数");
        assert_eq!(priced.quota_window.as_deref(), Some("month"), "窗口名归一后落库");
        assert!(priced.has_quota());

        // 过期是状态，不是禁用：行仍然 enabled，由 is_expired 决定能否过鉴权
        let stale = create(&pool, &CreateApiKeyRequest {
            name: "stale".to_string(),
            key: Some("sk-expiry-test-expiry-test-02".to_string()),
            proxy_id: None,
            expires_at: Some("2020-01-01T00:00:00Z".to_string()),
            quota_requests: Some(5),
            quota_tokens: None,
            quota_cost: None,
            quota_window: None,
        })
        .expect("create stale key");
        assert!(stale.is_expired(chrono::Utc::now()));
        assert!(stale.enabled);
        assert!(stale.to_public(KeyUsage { requests: 1, tokens: 9, cost: None, priced_requests: 0 }).expired);

        // 改回全局：Clear 让一把绑错的 Key 重新变成全局
        update(&pool, &created.id, &UpdateApiKeyRequest {
            name: None,
            enabled: None,
            proxy_id: NullableString::Clear,
            expires_at: NullableString::Keep,
            quota_requests: NullableNumber::Keep,
            quota_tokens: NullableNumber::Keep,
            quota_cost: NullableNumber::Keep,
            quota_window: NullableString::Clear,
        })
        .unwrap()
        .expect("rebound to global");
        let back = get(&pool, &created.id).unwrap().unwrap();
        assert_eq!(back.proxy_id, None);
        assert_eq!(back.quota_tokens, Some(1000), "Keep 不能顺手清掉别的列");
        assert_eq!(back.quota_cost, Some(12.5), "清窗口不该动金额");
        assert_eq!(back.quota_window, None);

        // 未知 id：不报错成"成功"，返回 None
        let missing = update(&pool, "no-such-key", &UpdateApiKeyRequest {
            name: Some("ghost".to_string()),
            enabled: None,
            proxy_id: NullableString::Keep,
            expires_at: NullableString::Keep,
            quota_requests: NullableNumber::Keep,
            quota_tokens: NullableNumber::Keep,
            quota_cost: NullableNumber::Keep,
            quota_window: NullableString::Keep,
        })
        .unwrap();
        assert!(missing.is_none());

        cleanup_db(&path);
    }

    /// 滚动窗口：同一批统计行，累计口径全算、日窗口只算最近这段。
    /// 列表（`usage_by_key` 按 Key 自己的窗口挑下界）与闸门（`usage_for_key`）必须给同一个分子。
    #[test]
    fn rolling_window_excludes_stats_outside_the_window() {
        use crate::models::api_key::{NullableString, UpdateApiKeyRequest};

        let (pool, path) = test_db();
        let key = create(&pool, &CreateApiKeyRequest {
            name: "daily".to_string(),
            key: Some("sk-window-test-window-test-01".to_string()),
            proxy_id: None,
            expires_at: None,
            quota_requests: None,
            quota_tokens: None,
            quota_cost: Some(2.0),
            quota_window: Some("day".to_string()),
        })
        .expect("create daily key");

        record_stat(&pool, &key.id, 10, 10, Some(1.0));
        record_stat(&pool, &key.id, 20, 20, Some(3.0));
        record_stat_at(&pool, &key.id, 100, 100, Some(9.0), "2020-01-01T00:00:00+00:00");

        let in_window = usage_for_key(&pool, &key.id, Some("day")).unwrap();
        assert_eq!(in_window.requests, 2, "窗口外的行不进分子");
        assert_eq!(in_window.tokens, 60);
        assert_eq!(in_window.cost, Some(4.0));
        assert_eq!(in_window.priced_requests, 2);

        let lifetime = usage_for_key(&pool, &key.id, None).unwrap();
        assert_eq!(lifetime.requests, 3, "累计口径仍算全部");
        assert_eq!(lifetime.cost, Some(13.0));

        // 列表按每把 Key 自己的 quota_window 挑下界，所以两处数字一致
        assert_eq!(usage_by_key(&pool).unwrap().get(&key.id).copied(), Some(in_window));

        // 窗口清掉后回到累计口径
        update(&pool, &key.id, &UpdateApiKeyRequest {
            quota_window: NullableString::Clear,
            ..Default::default()
        })
        .unwrap()
        .expect("window cleared");
        assert_eq!(usage_by_key(&pool).unwrap().get(&key.id).copied(), Some(lifetime));

        cleanup_db(&path);
    }

    /// 往 request_stats 塞一行，用来喂配额用量——口径必须与列表显示的完全一致。
    fn record_stat(pool: &DbPool, api_key_id: &str, token_input: i64, token_output: i64, cost: Option<f64>) {
        record_stat_at(pool, api_key_id, token_input, token_output, cost, &chrono::Utc::now().to_rfc3339());
    }

    fn record_stat_at(pool: &DbPool, api_key_id: &str, token_input: i64, token_output: i64, cost: Option<f64>, created_at: &str) {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, created_at, is_stream, api_key_id, cost)
             VALUES ('p','r','b',200,10,?1,?2,?3,0,?4,?5)",
            rusqlite::params![token_input, token_output, created_at, api_key_id, cost],
        )
        .unwrap();
    }

    /// 批量动作只碰点到的那几把，且"点了几把"与"改了几把"必须分开回。
    /// 界面会说"已停用 3 个"，如果其中一把早就不在了，这句话就得有出处。
    #[test]
    fn batch_actions_touch_only_the_selected_keys() {
        let (pool, path) = test_db();
        let proxy = crate::db::proxy::create(&pool, &CreateProxyRequest { name: "batch-target".to_string() })
            .expect("create proxy");

        let bound = create(&pool, &CreateApiKeyRequest {
            name: "bound".to_string(),
            key: Some("sk-aigw-batch-bound-bound-bound-01".to_string()),
            proxy_id: Some(proxy.id.clone()),
            quota_requests: Some(50),
            ..Default::default()
        })
        .expect("create bound key");
        let loose = create(&pool, &CreateApiKeyRequest {
            name: "loose".to_string(),
            key: Some("sk-aigw-batch-loose-loose-loose-02".to_string()),
            proxy_id: None,
            ..Default::default()
        })
        .expect("create loose key");
        let kept = create(&pool, &CreateApiKeyRequest {
            name: "kept".to_string(),
            key: Some("sk-aigw-batch-kept-kept-kept-03".to_string()),
            proxy_id: None,
            ..Default::default()
        })
        .expect("create kept key");
        record_stat(&pool, &loose.id, 10, 5, None);

        let injection = "x'); DROP TABLE api_keys;--".to_string();
        let ids = [bound.id.clone(), loose.id.clone(), injection.clone()];
        let (affected, missing) = batch_set_enabled(&pool, &ids, false).expect("batch disable");
        assert_eq!(affected, 2, "两把存在，注入串不算");
        assert_eq!(missing, vec![injection.clone()], "不存在的 id 要原样回给调用方");

        let after = list(&pool).expect("list");
        let bound_after = after.iter().find(|k| k.id == bound.id).expect("bound key 还在");
        assert!(!bound_after.enabled);
        assert!(!after.iter().find(|k| k.id == loose.id).expect("loose key 还在").enabled);
        assert!(after.iter().find(|k| k.id == kept.id).expect("kept key 还在").enabled, "没点到的不受影响");
        // 只改 enabled：名称、绑定与配额原封不动，否则批量停用会变成一次隐形重配置
        assert_eq!(bound_after.name, "bound");
        assert_eq!(bound_after.proxy_id.as_deref(), Some(proxy.id.as_str()));
        assert_eq!(bound_after.quota_requests, Some(50));

        let (affected, _) = batch_set_enabled(&pool, &[bound.id.clone()], true).expect("batch enable");
        assert_eq!(affected, 1);
        assert!(list(&pool).unwrap().iter().find(|k| k.id == bound.id).unwrap().enabled);

        let (affected, missing) = batch_delete(&pool, &ids).expect("batch delete");
        assert_eq!(affected, 2);
        assert_eq!(missing, [injection.as_str()]);
        let left = list(&pool).expect("list after delete");
        assert_eq!(left.len(), 1, "只删掉点到的两把");
        assert_eq!(left[0].id, kept.id);

        // 历史不跟着走：api_key_id 没有外键，删 Key 不该抹掉请求记录
        let conn = pool.get().unwrap();
        let stats: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_stats WHERE api_key_id=?1", [&loose.id], |r| r.get(0))
            .unwrap();
        assert_eq!(stats, 1, "删除 Key 后历史请求仍在，只是不再对应任何名称");
        let table: i64 = conn
            .query_row("SELECT COUNT(*) FROM api_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(table, 1, "注入串动不了表");

        cleanup_db(&path);
    }
}
