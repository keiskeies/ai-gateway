use rusqlite::{Connection, Row};
use serde::de::DeserializeOwned;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::config_transfer::*;
use crate::models::model::{Capability, ModelStatus};
use crate::models::platform::{PlatformStatus, PlatformType};
use crate::models::route::{
    BackendStatus, LBStrategy, ReasoningEcho, ReasoningStyle, RetryPolicy, UnknownParams,
};

/// 导出的是"配置"，不是运行数据：六张表就是全部，统计、健康历史、账号与会话都不在这里。
const PLATFORM_COLS: &str = "id, name, type, base_url, api_key, organization, custom_headers, \
    status, rate_limit, credit_budget, credit_window, catalog_key, created_at, updated_at";
const MODEL_COLS: &str = "id, platform_id, model_id, display_name, max_tokens, context_window, \
    input_price, output_price, cache_read_price, cache_write_price, credit_price, peak_pricing, \
    catalog_key, capabilities, status";
const ROUTE_COLS: &str = "id, proxy_id, lb_strategy, retry_policy, fallback, default_max_tokens, request_nonce";
const BACKEND_COLS: &str = "id, route_id, platform_id, model_id, weight, priority, max_concurrent, \
    capabilities, status, param_overrides, unknown_params, reasoning_style, reasoning_echo";
const API_KEY_COLS: &str = "id, name, key_hash, key_mask, proxy_id, created_at, last_used, \
    enabled, expires_at, quota_requests, quota_tokens, quota_cost, quota_window";

/// 文本列里存的是 JSON（枚举带引号、对象与数组原样）。
/// 解码口径与其余读取路径一致：能用默认值兜住的列，导出没资格因为同一处不一致而拒绝整份备份——
/// 那会让一个照常服务的库变成"导不出来"。
fn decode<T: DeserializeOwned>(raw: String, fallback: T) -> T {
    serde_json::from_str(&raw).unwrap_or(fallback)
}

fn decode_opt<T: DeserializeOwned>(raw: Option<String>) -> Option<T> {
    raw.and_then(|s| serde_json::from_str(&s).ok())
}

fn encode<T: serde::Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value).map_err(|e| AppError::Internal(e.to_string()))
}

pub fn export(pool: &DbPool, include_secrets: bool) -> AppResult<ConfigDocument> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    export_conn(&conn, include_secrets)
}

fn export_conn(conn: &Connection, include_secrets: bool) -> AppResult<ConfigDocument> {
    let mut doc = ConfigDocument {
        format: Some(CONFIG_FORMAT.to_string()),
        version: Some(CONFIG_VERSION),
        exported_at: chrono::Utc::now().to_rfc3339(),
        gateway_version: env!("CARGO_PKG_VERSION").to_string(),
        settings: None,
        ..Default::default()
    };

    let sql = format!("SELECT {PLATFORM_COLS} FROM platforms ORDER BY created_at, id");
    let mut stmt = conn.prepare(&sql)?;
    for row in stmt.query_map([], |row| map_platform(row, include_secrets))? {
        doc.platforms.push(row?);
    }
    drop(stmt);

    let sql = format!("SELECT {MODEL_COLS} FROM models ORDER BY platform_id, model_id");
    let mut stmt = conn.prepare(&sql)?;
    for row in stmt.query_map([], |row| Ok(ModelRow {
        id: row.get(0)?,
        platform_id: row.get(1)?,
        model_id: row.get(2)?,
        display_name: row.get(3)?,
        max_tokens: row.get::<_, i64>(4)?.max(0) as u32,
        context_window: row.get::<_, i64>(5)?.max(0) as u32,
        input_price: row.get(6)?,
        output_price: row.get(7)?,
        cache_read_price: row.get(8)?,
        cache_write_price: row.get(9)?,
        credit_price: row.get(10)?,
        peak_pricing: row.get::<_, Option<String>>(11)?
            .and_then(|raw| serde_json::from_str(&raw).ok()),
        catalog_key: row.get(12)?,
        capabilities: decode::<Vec<Capability>>(row.get::<_, String>(13)?, vec![]),
        status: decode(row.get::<_, String>(14)?, ModelStatus::Active),
    }))? {
        doc.models.push(row?);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id, name, created_at FROM proxies ORDER BY created_at, id")?;
    for row in stmt.query_map([], |row| Ok(ProxyRow {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
    }))? {
        doc.proxies.push(row?);
    }
    drop(stmt);

    let sql = format!("SELECT {ROUTE_COLS} FROM routes ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    for row in stmt.query_map([], |row| Ok(RouteRow {
        id: row.get(0)?,
        proxy_id: row.get(1)?,
        lb_strategy: decode(row.get::<_, String>(2)?, LBStrategy::RoundRobin),
        retry_policy: decode(row.get::<_, String>(3)?, RetryPolicy::default()),
        fallback: row.get(4)?,
        default_max_tokens: row.get::<_, Option<i64>>(5)?.map(|v| v.max(0) as u32),
        request_nonce: row.get::<_, i64>(6)? != 0,
    }))? {
        doc.routes.push(row?);
    }
    drop(stmt);

    let sql = format!("SELECT {BACKEND_COLS} FROM backends ORDER BY route_id, priority, weight, id");
    let mut stmt = conn.prepare(&sql)?;
    for row in stmt.query_map([], |row| Ok(BackendRow {
        id: row.get(0)?,
        route_id: row.get(1)?,
        platform_id: row.get(2)?,
        model_id: row.get(3)?,
        weight: row.get::<_, i64>(4)?.max(0) as u32,
        priority: row.get::<_, i64>(5)?.max(0) as u32,
        max_concurrent: row.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u32),
        capabilities: decode::<Vec<String>>(row.get::<_, String>(7)?, vec![]),
        status: decode(row.get::<_, String>(8)?, BackendStatus::Active),
        param_overrides: decode_opt(row.get::<_, Option<String>>(9)?)
            .unwrap_or_else(|| serde_json::json!({})),
        unknown_params: decode(row.get::<_, String>(10)?, UnknownParams::Auto),
        reasoning_style: decode(row.get::<_, String>(11)?, ReasoningStyle::Auto),
        reasoning_echo: decode(row.get::<_, String>(12)?, ReasoningEcho::Drop),
    }))? {
        doc.backends.push(row?);
    }
    drop(stmt);

    let sql = format!("SELECT {API_KEY_COLS} FROM api_keys ORDER BY created_at, id");
    let mut stmt = conn.prepare(&sql)?;
    for row in stmt.query_map([], |row| Ok(ApiKeyRow {
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
    }))? {
        doc.api_keys.push(row?);
    }

    Ok(doc)
}

fn map_platform(row: &Row<'_>, include_secrets: bool) -> rusqlite::Result<PlatformRow> {
    let raw_key: String = row.get(4)?;
    Ok(PlatformRow {
        id: row.get(0)?,
        name: row.get(1)?,
        platform_type: decode(row.get::<_, String>(2)?, PlatformType::OpenAI),
        base_url: row.get(3)?,
        // 默认脱敏：字段整个缺席，而不是塞一个掩码——掩码会被当成"这就是密钥"再导回去
        api_key: include_secrets.then_some(raw_key),
        organization: row.get(5)?,
        custom_headers: decode(row.get::<_, String>(6)?, serde_json::json!({})),
        status: decode(row.get::<_, String>(7)?, PlatformStatus::Active),
        rate_limit: decode_opt(row.get::<_, Option<String>>(8)?),
        credit_budget: row.get(9)?,
        credit_window: row.get(10)?,
        catalog_key: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

/// 库里现状：每张表按 id 索引，另附各自的 UNIQUE 取值。计划与落共用同一份快照。
#[derive(Default)]
struct Existing {
    platform_ids: std::collections::HashSet<String>,
    model_ids: std::collections::HashSet<String>,
    model_pairs: std::collections::HashSet<(String, String)>,
    model_catalog_pairs: std::collections::HashSet<(String, String)>,
    proxy_ids: std::collections::HashSet<String>,
    proxy_names: std::collections::HashSet<String>,
    route_ids: std::collections::HashSet<String>,
    route_proxies: std::collections::HashSet<String>,
    backend_ids: std::collections::HashSet<String>,
    key_ids: std::collections::HashSet<String>,
    key_hashes: std::collections::HashSet<String>,
}

/// 分隔符用 `\x1f`：id 与模型名都可能含空格，用逗号切会切错。
const SEP: char = '\u{1f}';

fn existing(conn: &Connection) -> AppResult<Existing> {
    let mut out = Existing::default();

    let mut stmt = conn.prepare("SELECT id, name FROM platforms")?;
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        out.platform_ids.insert(row?);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id, platform_id || char(31) || model_id FROM models")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, pair) = row?;
        out.model_ids.insert(id);
        let mut parts = pair.splitn(2, SEP);
        let platform = parts.next().unwrap_or_default().to_string();
        let model = parts.next().unwrap_or_default().to_string();
        out.model_pairs.insert((platform, model));
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT catalog_key, model_id FROM models WHERE catalog_key IS NOT NULL")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        out.model_catalog_pairs.insert(row?);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id, name FROM proxies")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        out.proxy_ids.insert(id);
        out.proxy_names.insert(name);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id, proxy_id FROM routes")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, proxy_id) = row?;
        out.route_ids.insert(id);
        out.route_proxies.insert(proxy_id);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id FROM backends")?;
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        out.backend_ids.insert(row?);
    }
    drop(stmt);

    let mut stmt = conn.prepare("SELECT id, key_hash FROM api_keys")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, hash) = row?;
        out.key_ids.insert(id);
        out.key_hashes.insert(hash);
    }

    Ok(out)
}

/// 一行落在哪一格：覆盖模式下同一 id 是"由文档版本取代"，增量模式下是"不动"。
fn count(table: &mut TablePlan, mode: ImportMode, known: bool) {
    match (mode, known) {
        (ImportMode::Replace, true) => table.overwrite += 1,
        (_, true) => table.skip += 1,
        (_, false) => table.add += 1,
    }
}

/// 影响预演：只读，不写。两种模式共用一套判断，apply 也照同样条件跳过，
/// 这样"计划里看到的"与"真正落库的"不会各说各话。
pub fn plan(conn: &Connection, doc: &ConfigDocument, mode: ImportMode) -> AppResult<ImportPlan> {
    let current = existing(conn)?;
    let merge = mode == ImportMode::Merge;
    let mut plan = ImportPlan {
        mode,
        source_version: doc.gateway_version.clone(),
        source_exported_at: doc.exported_at.clone(),
        errors: doc.check(),
        warnings: doc.notes(),
        ..Default::default()
    };

    if !merge {
        let keeps = |ids: &std::collections::HashSet<String>, keep: &std::collections::HashSet<&str>| {
            ids.iter().filter(|i| !keep.contains(i.as_str())).count()
        };
        let doc_platforms: std::collections::HashSet<&str> =
            doc.platforms.iter().map(|r| r.id.as_str()).collect();
        let doc_models: std::collections::HashSet<&str> =
            doc.models.iter().map(|r| r.id.as_str()).collect();
        let doc_proxies: std::collections::HashSet<&str> =
            doc.proxies.iter().map(|r| r.id.as_str()).collect();
        let doc_routes: std::collections::HashSet<&str> =
            doc.routes.iter().map(|r| r.id.as_str()).collect();
        let doc_backends: std::collections::HashSet<&str> =
            doc.backends.iter().map(|r| r.id.as_str()).collect();
        let doc_keys: std::collections::HashSet<&str> =
            doc.api_keys.iter().map(|r| r.id.as_str()).collect();
        plan.platforms.drop = keeps(&current.platform_ids, &doc_platforms);
        plan.models.drop = keeps(&current.model_ids, &doc_models);
        plan.proxies.drop = keeps(&current.proxy_ids, &doc_proxies);
        plan.routes.drop = keeps(&current.route_ids, &doc_routes);
        plan.backends.drop = keeps(&current.backend_ids, &doc_backends);
        plan.api_keys.drop = keeps(&current.key_ids, &doc_keys);
        let dropped = plan.platforms.drop + plan.models.drop + plan.proxies.drop
            + plan.routes.drop + plan.backends.drop + plan.api_keys.drop;
        if dropped > 0 {
            plan.warnings.insert(
                0,
                Issue::new(
                    "cfgReplaceDrops",
                    [
                        plan.platforms.drop.to_string(),
                        plan.models.drop.to_string(),
                        plan.proxies.drop.to_string(),
                        plan.routes.drop.to_string(),
                        plan.backends.drop.to_string(),
                        plan.api_keys.drop.to_string(),
                    ],
                ),
            );
        }
    }

    for row in &doc.platforms {
        count(&mut plan.platforms, mode, current.platform_ids.contains(&row.id));
    }
    for row in &doc.models {
        let known = current.model_ids.contains(&row.id);
        // 同一 (平台, 模型) 已经挂在别的 id 下：插进去会撞 UNIQUE。这只是冗余行，
        // 跳过并说明，不该让整个导入失败。
        let taken = merge
            && !known
            && current.model_pairs.contains(&(row.platform_id.clone(), row.model_id.clone()));
        if taken {
            plan.models.skip += 1;
            plan.warnings.push(Issue::new("cfgModelPairTaken", [row.model_id.clone(), row.platform_id.clone()]));
        } else {
            count(&mut plan.models, mode, known);
        }
    }
    for row in &doc.proxies {
        let known = current.proxy_ids.contains(&row.id);
        // 名字是 UNIQUE：同名不同 id 既插不进去，也不能悄悄改指另一个虚拟模型——那等于把历史改道
        if merge && !known && current.proxy_names.contains(&row.name) {
            plan.proxies.skip += 1;
            plan.errors.push(Issue::new("cfgProxyNameTaken", [row.name.clone()]));
        } else {
            count(&mut plan.proxies, mode, known);
        }
    }
    for row in &doc.routes {
        let known = current.route_ids.contains(&row.id);
        if merge && !known && current.route_proxies.contains(&row.proxy_id) {
            plan.routes.skip += 1;
            plan.errors.push(Issue::new("cfgRouteTaken", [row.proxy_id.clone(), row.id.clone()]));
        } else {
            count(&mut plan.routes, mode, known);
        }
    }
    for row in &doc.backends {
        count(&mut plan.backends, mode, current.backend_ids.contains(&row.id));
    }
    for row in &doc.api_keys {
        let known = current.key_ids.contains(&row.id);
        // 摘要撞车＝同一把凭据本机已以另一个名字存在：插进去撞 UNIQUE，忽略则两把 Key 变成一个
        if merge && !known && current.key_hashes.contains(&row.key_hash) {
            plan.api_keys.skip += 1;
            plan.errors.push(Issue::new("cfgKeyHashTaken", [row.name.clone()]));
        } else {
            count(&mut plan.api_keys, mode, known);
        }
    }

    if doc.is_empty() {
        plan.warnings.push(Issue::new(
            if merge { "cfgEmptyMerge" } else { "cfgEmptyReplace" },
            [] as [String; 0],
        ));
    }

    plan.ok = plan.errors.is_empty();
    Ok(plan)
}

/// 落库：一个事务内先按模式清空再整体插入；任何一步失败就回滚，
/// 不允许出现"平台进了一半、路由全部失踪"的配置。
pub fn apply(conn: &mut Connection, doc: &ConfigDocument, mode: ImportMode) -> AppResult<ImportResult> {
    let plan = plan(conn, doc, mode)?;
    if !plan.ok {
        let detail = plan.errors.iter().map(|i| format!("{} {:?}", i.code, i.args)).collect::<Vec<_>>().join(" / ");
        return Err(AppError::BadRequest(format!("Import rejected: {detail}")));
    }
    // 计划读过一次现状；这里重新读，避免两次调用之间库被别人写过
    let current = existing(conn)?;
    let merge = mode == ImportMode::Merge;

    let tx = conn.transaction().map_err(|e| AppError::Internal(e.to_string()))?;
    if !merge {
        // 子表先删：backends 对 platforms 的外键没有 CASCADE，靠顺序保证
        for table in ["api_keys", "backends", "routes", "proxies", "models", "platforms"] {
            let sql = format!("DELETE FROM {table}");
            tx.execute(&sql, [])?;
        }
    }
    let mut result = ImportResult { mode, ..Default::default() };

    for row in &doc.platforms {
        if merge && current.platform_ids.contains(&row.id) {
            continue;
        }
        let catalog_key = row.catalog_key.clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| crate::catalog::catalog_key(&row.platform_type, &row.base_url));
        tx.execute(
            &format!("INSERT INTO platforms ({PLATFORM_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"),
            rusqlite::params![
                row.id,
                row.name,
                encode(&row.platform_type)?,
                row.base_url,
                // 脱敏导出导入后是空密钥：这一行仍然可用，只差用户补一次凭据
                row.api_key.clone().unwrap_or_default(),
                row.organization,
                encode(&row.custom_headers)?,
                encode(&row.status)?,
                row.rate_limit.as_ref().map(encode).transpose()?,
                row.credit_budget,
                row.credit_window,
                catalog_key,
                row.created_at,
                row.updated_at,
            ],
        )?;
        result.platforms.add += 1;
    }

    let platform_keys: std::collections::HashMap<String, String> = {
        let mut stmt = tx.prepare("SELECT id, catalog_key FROM platforms WHERE catalog_key IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(|row| row.ok()).collect()
    };
    let mut seen_catalog = std::collections::HashSet::new();
    for row in &doc.models {
        let Some(catalog_key) = row.catalog_key.clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| platform_keys.get(&row.platform_id).cloned())
        else {
            continue;
        };
        let catalog_pair = (catalog_key.clone(), row.model_id.clone());
        if !seen_catalog.insert(catalog_pair.clone()) {
            continue;
        }
        if merge && (current.model_ids.contains(&row.id) || current.model_catalog_pairs.contains(&catalog_pair)) {
            continue;
        }
        let peak_pricing = row.peak_pricing.as_ref().map(serde_json::to_string).transpose()?;
        let changed = tx.execute(
            &format!("INSERT OR IGNORE INTO models ({MODEL_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"),
            rusqlite::params![
                row.id,
                row.platform_id,
                row.model_id,
                row.display_name,
                row.max_tokens as i64,
                row.context_window as i64,
                row.input_price,
                row.output_price,
                row.cache_read_price,
                row.cache_write_price,
                row.credit_price,
                peak_pricing,
                catalog_key,
                encode(&row.capabilities)?,
                encode(&row.status)?,
            ],
        )?;
        if changed > 0 {
            result.models.add += 1;
        }
    }
    for row in &doc.proxies {
        if merge && current.proxy_ids.contains(&row.id) {
            continue;
        }
        tx.execute(
            "INSERT INTO proxies (id, name, created_at) VALUES (?1,?2,?3)",
            rusqlite::params![row.id, row.name, row.created_at],
        )?;
        result.proxies.add += 1;
    }
    for row in &doc.routes {
        if merge && current.route_ids.contains(&row.id) {
            continue;
        }
        tx.execute(
            &format!("INSERT INTO routes ({ROUTE_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7)"),
            rusqlite::params![
                row.id,
                row.proxy_id,
                encode(&row.lb_strategy)?,
                encode(&row.retry_policy)?,
                row.fallback,
                row.default_max_tokens.map(|v| v as i64),
                row.request_nonce as i64,
            ],
        )?;
        result.routes.add += 1;
    }
    for row in &doc.backends {
        if merge && current.backend_ids.contains(&row.id) {
            continue;
        }
        tx.execute(
            &format!("INSERT INTO backends ({BACKEND_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"),
            rusqlite::params![
                row.id,
                row.route_id,
                row.platform_id,
                row.model_id,
                row.weight as i64,
                row.priority as i64,
                row.max_concurrent.map(|v| v as i64),
                encode(&row.capabilities)?,
                encode(&row.status)?,
                encode(&row.param_overrides)?,
                encode(&row.unknown_params)?,
                encode(&row.reasoning_style)?,
                encode(&row.reasoning_echo)?,
            ],
        )?;
        result.backends.add += 1;
    }
    for row in &doc.api_keys {
        if merge && (current.key_ids.contains(&row.id) || current.key_hashes.contains(&row.key_hash)) {
            continue;
        }
        if row.key_hash.is_empty() {
            continue;
        }
        let mask = if row.key_mask.is_empty() { redact_hint(&row.key_hash) } else { row.key_mask.clone() };
        tx.execute(
            &format!("INSERT INTO api_keys ({API_KEY_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"),
            rusqlite::params![
                row.id,
                row.name,
                row.key_hash,
                mask,
                row.proxy_id,
                row.created_at,
                row.last_used,
                row.enabled as i64,
                row.expires_at,
                row.quota_requests,
                row.quota_tokens,
                row.quota_cost.filter(|v| *v > 0.0),
                // 手写的导入文件里窗口可以写歪；归一失败退成累计，而不是存一个谁也认不出的字符串
                crate::models::api_key::canonical_quota_window(row.quota_window.as_deref()),
            ],
        )?;
        result.api_keys.add += 1;
    }

    tx.commit().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(result)
}

/// 掩码列是 NOT NULL：手写的导入文件没带掩码时，从摘要自己造一个只用于认读的占位，
/// 而不是留空——列表里空白的凭据列会让人以为 Key 没导进来。
fn redact_hint(hash: &str) -> String {
    format!("••••{}", hash.chars().take(8).collect::<String>())
}

pub fn plan_pool(pool: &DbPool, doc: &ConfigDocument, mode: ImportMode) -> AppResult<ImportPlan> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    plan(&conn, doc, mode)
}

pub fn apply_pool(pool: &DbPool, doc: &ConfigDocument, mode: ImportMode) -> AppResult<ImportResult> {
    let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    apply(&mut conn, doc, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    fn test_db(tag: &str) -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("config-{}-{}.db", tag, uuid::Uuid::new_v4()));
        let pool = init_pool(&path).expect("init test db");
        (pool, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    /// 凭据列存的是 sha256 hex，`check()` 会验形状（64 位），测试数据必须和真摘要同形。
    const KEY_HASH: &str = "0f1e2d3c4b5a69780f1e2d3c4b5a69780f1e2d3c4b5a69780f1e2d3c4b5a6978";

    fn seed(pool: &DbPool) {
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO platforms (id,name,type,base_url,api_key,organization,custom_headers,status,rate_limit,credit_budget,credit_window,created_at,updated_at)
               VALUES ('p1','本地','\"OpenAI\"','http://127.0.0.1:8080/v1','sk-secret',NULL,'{\"X-A\":\"1\"}','\"Active\"',NULL,250,'day','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
             INSERT INTO models (id,platform_id,model_id,display_name,max_tokens,context_window,input_price,output_price,credit_price,capabilities,status)
               VALUES ('m1','p1','llama','llama',8192,32768,0.5,1.2,2,'[\"Vision\",\"Reasoning\"]','\"Active\"');
             INSERT INTO proxies (id,name,created_at) VALUES ('x1','chat','2026-01-01T00:00:00Z');
             INSERT INTO routes (id,proxy_id,lb_strategy,retry_policy,fallback,default_max_tokens)
               VALUES ('r1','x1','\"WeightedRandom\"','{\"max_retries\":3,\"retry_on_error\":[\"Timeout\"],\"backoff_ms\":250}',NULL,2048);
             INSERT INTO backends (id,route_id,platform_id,model_id,weight,priority,max_concurrent,capabilities,status,param_overrides,unknown_params,reasoning_style,reasoning_echo)
               VALUES ('b1','r1','p1','llama',3,1,NULL,'[]','\"Active\"','{\"temperature\":0.2}','\"Drop\"','\"AnthropicThinking\"','\"Keep\"');
             INSERT INTO api_keys (id,name,key_hash,key_mask,proxy_id,created_at,last_used,enabled,expires_at,quota_requests,quota_tokens,quota_cost,quota_window)
               VALUES ('k1','prod','0f1e2d3c4b5a69780f1e2d3c4b5a69780f1e2d3c4b5a69780f1e2d3c4b5a6978','sk-p••••••••oc','x1','2026-01-02T00:00:00Z',NULL,1,'2027-01-01T23:59:59+00:00',100,NULL,12.5,'month');",
        )
        .unwrap();
        let catalog_key = crate::catalog::catalog_key(
            &crate::models::platform::PlatformType::OpenAI,
            "http://127.0.0.1:8080/v1",
        );
        conn.execute("UPDATE platforms SET catalog_key = ?1 WHERE id = 'p1'", [&catalog_key]).unwrap();
        conn.execute("UPDATE models SET catalog_key = ?1 WHERE platform_id = 'p1'", [&catalog_key]).unwrap();
    }

    /// 导出→清空→导入必须回到同一个形状：备份功能的最低要求。
    /// 逐列比对靠"两次导出的文档相等"，比手写几十条断言更少漏项。
    #[test]
    fn export_import_round_trip_restores_every_column() {
        let (pool, path) = test_db("round-trip");
        seed(&pool);
        let exported = export(&pool, true).expect("export");
        assert_eq!(exported.platforms[0].api_key.as_deref(), Some("sk-secret"));
        assert_eq!(exported.platforms[0].credit_budget, Some(250.0));
        assert_eq!(exported.platforms[0].credit_window.as_deref(), Some("day"));
        assert_eq!(exported.models[0].credit_price, Some(2.0));
        assert_eq!(
            exported.platforms[0].custom_headers,
            serde_json::json!({"X-A": "1"})
        );
        assert_eq!(exported.models[0].capabilities, vec![Capability::Vision, Capability::Reasoning]);
        assert_eq!(exported.models[0].context_window, 32768);
        assert_eq!(exported.routes[0].lb_strategy, LBStrategy::WeightedRandom);
        assert_eq!(exported.routes[0].retry_policy.max_retries, 3);
        assert_eq!(exported.routes[0].default_max_tokens, Some(2048));
        assert_eq!(exported.backends[0].reasoning_style, ReasoningStyle::AnthropicThinking);
        assert_eq!(exported.backends[0].unknown_params, UnknownParams::Drop);
        assert_eq!(exported.backends[0].param_overrides, serde_json::json!({"temperature": 0.2}));
        assert_eq!(exported.api_keys[0].quota_requests, Some(100));
        assert_eq!(exported.api_keys[0].quota_cost, Some(12.5));
        assert_eq!(exported.api_keys[0].quota_window.as_deref(), Some("month"));
        assert_eq!(exported.api_keys[0].expires_at.as_deref(), Some("2027-01-01T23:59:59+00:00"));

        let snapshot = serde_json::to_string(&exported).unwrap();
        {
            let mut conn = pool.get().unwrap();
            apply(&mut conn, &exported, ImportMode::Replace).expect("apply");
        }
        let mut again = export(&pool, true).expect("re-export");
        // 只有导出时间戳会变，它不属于配置内容
        again.exported_at = exported.exported_at.clone();
        assert_eq!(snapshot, serde_json::to_string(&again).unwrap());
        cleanup_db(&path);
    }

    /// 默认导出脱敏：文件里没有 api_key 字段；增量导入也不许覆盖已有密钥
    #[test]
    fn default_export_omits_secrets_and_merge_keeps_them() {
        let (pool, path) = test_db("redact");
        seed(&pool);
        let doc = export(&pool, false).unwrap();
        let text = serde_json::to_string(&doc).unwrap();
        assert!(!text.contains("sk-secret"), "脱敏导出不能带走明文密钥");
        assert!(!text.contains("\"api_key\""), "字段要整个缺席，不是空串");
        assert!(doc.notes().iter().any(|n| n.code == "cfgRedactedKeys"));

        let plan = plan_pool(&pool, &doc, ImportMode::Merge).unwrap();
        assert!(plan.ok, "{:?}", plan.errors);
        assert_eq!(plan.platforms.skip, 1, "已有 id 在增量模式下不动");

        apply_pool(&pool, &doc, ImportMode::Merge).unwrap();
        let conn = pool.get().unwrap();
        let still: String = conn
            .query_row("SELECT api_key FROM platforms WHERE id='p1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(still, "sk-secret", "增量导入不得覆盖已有密钥");
        cleanup_db(&path);
    }

    /// 覆盖导入要预告"删多少"，因为历史统计的归属会断
    #[test]
    fn replace_plan_reports_drops_and_overwrites() {
        let (pool, path) = test_db("replace-plan");
        seed(&pool);
        let mut doc = export(&pool, false).unwrap();
        doc.platforms[0].name = "改过名".to_string();
        // x1 一消失，挂在它身上的路由/后端/Key 必须一起从文档里撤掉——
        // 留着就是悬挂引用，那是阻断级错误，不是这次要测的东西
        let k1 = doc.api_keys.iter().find(|k| k.id == "k1").cloned().unwrap();
        doc.proxies.retain(|p| p.id != "x1");
        doc.routes.retain(|r| r.proxy_id != "x1");
        doc.backends.clear();
        doc.api_keys.clear();
        doc.api_keys.push(ApiKeyRow { proxy_id: None, ..k1 });
        let plan = plan_pool(&pool, &doc, ImportMode::Replace).unwrap();
        assert!(plan.ok, "{:?}", plan.errors);
        assert_eq!(plan.platforms.overwrite, 1);
        assert_eq!(plan.proxies.drop, 1);
        assert_eq!(plan.routes.drop, 1);
        assert!(plan.warnings.iter().any(|w| w.code == "cfgReplaceDrops"));

        apply_pool(&pool, &doc, ImportMode::Replace).unwrap();
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "文档里没有的虚拟模型被删掉");
        let name: String = conn
            .query_row("SELECT name FROM platforms WHERE id='p1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "改过名");
        // Key 的摘要不变，只是随撤掉的代理回落到全局——这正是 V14 之后绑定关系的可观测口径
        let (keys, global): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(proxy_id IS NULL) FROM api_keys WHERE id='k1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((keys, global), (1, 1), "Key 随文档重建，绑定按文档更新");
        cleanup_db(&path);
    }

    /// 同名不同 id 的虚拟模型、摘要撞车的 Key：既不静默插入也不静默跳过
    #[test]
    fn merge_refuses_silent_identity_collisions() {
        let (pool, path) = test_db("collide");
        seed(&pool);
        let mut doc = ConfigDocument {
            format: Some(CONFIG_FORMAT.to_string()),
            version: Some(CONFIG_VERSION),
            ..Default::default()
        };
        doc.proxies.push(ProxyRow {
            id: "x2".to_string(),
            name: "chat".to_string(),
            created_at: "2026-02-01T00:00:00Z".to_string(),
        });
        doc.api_keys.push(ApiKeyRow {
            id: "k2".to_string(),
            name: "dup".to_string(),
            key_hash: KEY_HASH.to_string(),
            key_mask: String::new(),
            proxy_id: None,
            enabled: true,
            expires_at: None,
            quota_requests: None,
            quota_tokens: None,
            quota_cost: None,
            quota_window: None,
            created_at: "2026-02-01T00:00:00Z".to_string(),
            last_used: None,
        });
        let plan = plan_pool(&pool, &doc, ImportMode::Merge).unwrap();
        let codes: Vec<&str> = plan.errors.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains(&"cfgProxyNameTaken"), "{codes:?}");
        assert!(codes.contains(&"cfgKeyHashTaken"), "{codes:?}");
        // 挡住之后一行都不写
        let error = apply_pool(&pool, &doc, ImportMode::Merge).unwrap_err();
        assert!(matches!(error, AppError::BadRequest(_)), "{error:?}");
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM proxies WHERE id='x2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        cleanup_db(&path);
    }

    /// 同一 (平台, 模型) 已经挂在别的 id 下：跳过并说明就好，不该让整个导入失败
    #[test]
    fn model_pair_collision_skips_instead_of_failing() {
        let (pool, path) = test_db("dup-model");
        seed(&pool);
        // 用真实导出的骨架，只换掉模型行的 id：引用关系天然合法
        let mut doc = export(&pool, false).unwrap();
        doc.models[0].id = "m9".to_string();
        let plan = plan_pool(&pool, &doc, ImportMode::Merge).unwrap();
        assert!(plan.ok, "{:?}", plan.errors);
        assert_eq!(plan.models.skip, 1, "撞 (p1, llama) 的那行算跳过，不算新增");
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.code == "cfgModelPairTaken"
                && w.args == vec!["llama".to_string(), "p1".to_string()]),
            "{:?}",
            plan.warnings);

        apply_pool(&pool, &doc, ImportMode::Merge).unwrap();
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "撞 UNIQUE 的那行没有写进去");
        cleanup_db(&path);
    }

    /// 文档自己有两行同 (平台, 模型)：价格可能各说一套，阻断比"猜留哪一行"诚实
    #[test]
    fn in_document_duplicate_model_pair_blocks_the_import() {
        let (pool, path) = test_db("dup-in-doc");
        seed(&pool);
        let mut doc = export(&pool, false).unwrap();
        let mut extra = doc.models[0].clone();
        extra.id = "m9".to_string();
        doc.models.push(extra);
        let plan = plan_pool(&pool, &doc, ImportMode::Merge).unwrap();
        assert!(!plan.ok);
        assert!(plan
            .errors
            .iter()
            .any(|e| e.code == "cfgDuplicateModel" && e.args == vec!["llama".to_string(), "p1".to_string()]));
        assert!(matches!(
            apply_pool(&pool, &doc, ImportMode::Merge),
            Err(AppError::BadRequest(_))
        ));
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM models WHERE id='m9'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        cleanup_db(&path);
    }

    /// 坏文档在写库之前就被挡住，而且库里一切照旧
    #[test]
    fn invalid_document_writes_nothing() {
        let (pool, path) = test_db("invalid");
        seed(&pool);
        let mut doc = export(&pool, false).unwrap();
        doc.backends[0].platform_id = "ghost".to_string();
        let error = apply_pool(&pool, &doc, ImportMode::Merge).unwrap_err();
        assert!(matches!(error, AppError::BadRequest(_)), "{error:?}");
        assert_eq!(export(&pool, false).unwrap().backends[0].platform_id, "p1");
        cleanup_db(&path);
    }

    /// 空文档 + 覆盖＝一次有意的清空：允许，但必须警告
    #[test]
    fn empty_document_clears_only_on_replace() {
        let (pool, path) = test_db("empty");
        seed(&pool);
        let empty = ConfigDocument {
            format: Some(CONFIG_FORMAT.to_string()),
            version: Some(CONFIG_VERSION),
            ..Default::default()
        };
        let merge = plan_pool(&pool, &empty, ImportMode::Merge).unwrap();
        assert!(merge.ok && merge.platforms.add == 0);
        let replace = plan_pool(&pool, &empty, ImportMode::Replace).unwrap();
        assert!(replace.ok);
        assert_eq!(replace.platforms.drop, 1);
        apply_pool(&pool, &empty, ImportMode::Replace).unwrap();
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM platforms", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        cleanup_db(&path);
    }
}
