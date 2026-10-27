use crate::error::AppResult;
use crate::models::api_key::NullableString;
use crate::models::model::NullableNumber;
use crate::models::platform::*;
use crate::db::DbPool;

const PLATFORM_COLS: &str = "id, name, type, base_url, api_key, organization, custom_headers, status, \
    rate_limit, credit_budget, credit_window, catalog_key, created_at, updated_at, credit_unit, adapter";

fn map_platform(row: &rusqlite::Row) -> rusqlite::Result<Platform> {
    let rate_limit_str: Option<String> = row.get(8)?;
    Ok(Platform {
        id: row.get(0)?,
        name: row.get(1)?,
        platform_type: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(PlatformType::OpenAI),
        base_url: row.get(3)?,
        api_key: row.get(4)?,
        organization: row.get(5)?,
        custom_headers: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or(serde_json::Value::Object(Default::default())),
        status: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(PlatformStatus::Active),
        rate_limit: rate_limit_str.and_then(|s| serde_json::from_str(&s).ok()),
        credit_budget: row.get(9)?,
        credit_window: row.get(10)?,
        catalog_key: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        credit_unit: row.get(14)?,
        adapter: row.get(15)?,
    })
}

pub fn list(pool: &DbPool) -> AppResult<Vec<Platform>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!("SELECT {PLATFORM_COLS} FROM platforms ORDER BY created_at"))?;
    let rows = stmt.query_map([], map_platform)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Platform> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row(
        &format!("SELECT {PLATFORM_COLS} FROM platforms WHERE id = ?1"),
        [id],
        map_platform,
    ).map_err(Into::into)
}

pub fn create(pool: &DbPool, req: &CreatePlatformRequest) -> AppResult<Platform> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // 额度单位先归一，再由它决定预算/窗口是否合法（credits 才允许带预算）。
    //
    // 向后兼容：V28 之前「有 credit_budget 就是积分平台」，没有 unit 这个概念。
    // 所以「没写 unit + 给了预算」必须落成 credits，而不是报错——否则升级后
    // 所有既有平台的编辑请求都会被拒。
    let credit_unit = resolve_create_unit(req.credit_unit.as_deref(), req.credit_budget);
    let credit_window = validate_credit_unit(
        credit_unit.as_deref(),
        req.credit_budget,
        req.credit_window.as_deref(),
    )
    .map_err(crate::error::AppError::BadRequest)?;
    // adapter 显式给了就校验，没给就留 NULL（读时由 platform_type 推出）
    let adapter = normalize_adapter(req.adapter.as_deref())
        .map_err(crate::error::AppError::BadRequest)?;
    let catalog_key = crate::catalog::catalog_key_with_adapter(
        &req.platform_type,
        &req.base_url,
        adapter.as_deref(),
    );
    let platform = Platform {
        id: id.clone(),
        name: req.name.clone(),
        platform_type: req.platform_type.clone(),
        base_url: req.base_url.clone(),
        api_key: req.api_key.clone(),
        organization: req.organization.clone(),
        custom_headers: req.custom_headers.clone(),
        status: PlatformStatus::Active,
        rate_limit: None,
        credit_budget: req.credit_budget,
        credit_window,
        credit_unit,
        adapter,
        catalog_key: Some(catalog_key.clone()),
        created_at: now.clone(),
        updated_at: now,
    };

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO platforms (id, name, type, base_url, api_key, organization, custom_headers, status, rate_limit, credit_budget, credit_window, catalog_key, created_at, updated_at, credit_unit, adapter) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            platform.id,
            platform.name,
            serde_json::to_string(&platform.platform_type)?,
            platform.base_url,
            platform.api_key,
            platform.organization,
            serde_json::to_string(&platform.custom_headers)?,
            serde_json::to_string(&platform.status)?,
            platform.rate_limit.as_ref().map(|r| serde_json::to_string(r).unwrap()),
            platform.credit_budget,
            platform.credit_window,
            catalog_key,
            platform.created_at,
            platform.updated_at,
            platform.credit_unit,
            platform.adapter,
        ],
    )?;
    Ok(platform)
}

/// 校验并归一 adapter 字符串：空串 / None → None（由 type 推出）；非法值报错。
fn normalize_adapter(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let parsed: crate::models::adapter::PlatformAdapter =
        serde_json::from_str(&format!("\"{}\"", raw))
            .map_err(|_| format!("adapter must be OpenAI, Anthropic, Trae or WorkBuddy: {raw}"))?;
    Ok(Some(format!("{:?}", parsed)))
}

/// 创建时的额度单位归一。
///
/// 关键的后向兼容规则：**没写 unit 但给了预算 = credits**。
/// V28 之前没有 unit 概念，「有 credit_budget 就是积分平台」是既有语义；
/// 若在这里要求显式声明，升级后所有存量平台的编辑/导入都会被拒。
fn resolve_create_unit(raw: Option<&str>, budget: Option<f64>) -> Option<String> {
    use crate::models::adapter::CreditUnit;
    let explicit = raw.map(str::trim).filter(|s| !s.is_empty());
    match explicit {
        Some(value) => {
            let parsed = CreditUnit::resolve(Some(value));
            if parsed == CreditUnit::None {
                None
            } else {
                Some(parsed.key_str().to_string())
            }
        }
        // 没写 unit：有预算就按 credits 处理（V24 语义），否则不计量
        None => budget.map(|_| CreditUnit::Credits.key_str().to_string()),
    }
}

/// 更新时的额度单位归一。三态语义：
/// - `Keep`（没写）：保持库里的值；若库里是 NULL 但这次给了预算，同样按 credits 处理
/// - `Clear`（null）：清空 → 必须同时清预算，否则报错
/// - `Set(v)`：按 v 解析
fn resolve_update_unit(
    field: &NullableString,
    current: Option<&str>,
    budget: Option<f64>,
) -> Result<Option<String>, String> {
    use crate::models::adapter::CreditUnit;
    match field {
        NullableString::Keep => {
            if let Some(existing) = current {
                return Ok(Some(existing.to_string()));
            }
            // 库里没有 unit：有预算就是老的 V24 积分平台
            Ok(budget.map(|_| CreditUnit::Credits.key_str().to_string()))
        }
        NullableString::Clear => Ok(None),
        NullableString::Set(value) => {
            let parsed = CreditUnit::resolve(Some(value.as_str()));
            if parsed == CreditUnit::None {
                Ok(None)
            } else {
                Ok(Some(parsed.key_str().to_string()))
            }
        }
    }
}

pub fn update(pool: &DbPool, id: &str, req: &UpdatePlatformRequest) -> AppResult<Platform> {
    let mut platform = get(pool, id)?;
    if let Some(name) = &req.name { platform.name = name.clone(); }
    if let Some(pt) = &req.platform_type { platform.platform_type = pt.clone(); }
    if let Some(url) = &req.base_url { platform.base_url = url.clone(); }
    if let Some(key) = &req.api_key { platform.api_key = key.clone(); }
    if let Some(org) = &req.organization { platform.organization = Some(org.clone()); }
    if let Some(hdrs) = &req.custom_headers { platform.custom_headers = hdrs.clone(); }
    if let Some(status) = &req.status { platform.status = status.clone(); }

    // 积分字段是三态：没写 = 不改，null = 关闭，给值 = 设值。
    // 先分别应用，再整体校验，避免“先清预算后写窗口”之类的中途状态落库。
    let budget = match req.credit_budget {
        NullableNumber::Keep => platform.credit_budget,
        NullableNumber::Clear => None,
        NullableNumber::Set(v) => Some(v),
    };
    let window = match &req.credit_window {
        NullableString::Keep => platform.credit_window.clone(),
        NullableString::Clear => None,
        NullableString::Set(v) => Some(v.clone()),
    };
    let unit = resolve_update_unit(
        &req.credit_unit,
        platform.credit_unit.as_deref(),
        budget,
    )
    .map_err(crate::error::AppError::BadRequest)?;
    // 换单位时旧预算可能不再适用：把单位也参与校验，而不是只校验预算与窗口。
    let window = validate_credit_unit(unit.as_deref(), budget, window.as_deref())
        .map_err(crate::error::AppError::BadRequest)?;
    // 单位降级到非 credits 时，预算必须一并清空（否则库里留下永远不生效的值）
    let budget = if crate::models::adapter::CreditUnit::resolve(unit.as_deref())
        .uses_credit_ledger()
    {
        budget
    } else {
        None
    };
    platform.credit_unit = unit;
    platform.credit_budget = budget;
    platform.credit_window = window;

    // adapter 三态：没写 = 不改，null = 清空（回落由 type 推出），字符串 = 设值
    let adapter = match &req.adapter {
        NullableString::Keep => platform.adapter.clone(),
        NullableString::Clear => None,
        NullableString::Set(v) => {
            normalize_adapter(Some(v.as_str())).map_err(crate::error::AppError::BadRequest)?
        }
    };
    platform.adapter = adapter;

    let old_catalog_key = platform.catalog_key.clone();
    let new_catalog_key = crate::catalog::catalog_key_with_adapter(
        &platform.platform_type,
        &platform.base_url,
        platform.adapter.as_deref(),
    );
    platform.catalog_key = Some(new_catalog_key.clone());
    platform.updated_at = chrono::Utc::now().to_rfc3339();

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platforms SET name=?2, type=?3, base_url=?4, api_key=?5, organization=?6, custom_headers=?7, status=?8, credit_budget=?9, credit_window=?10, catalog_key=?11, updated_at=?12, credit_unit=?13, adapter=?14 WHERE id=?1",
        rusqlite::params![
            id, platform.name, serde_json::to_string(&platform.platform_type)?,
            platform.base_url, platform.api_key, platform.organization,
            serde_json::to_string(&platform.custom_headers)?,
            serde_json::to_string(&platform.status)?,
            platform.credit_budget, platform.credit_window, platform.catalog_key, platform.updated_at,
            platform.credit_unit, platform.adapter,
        ],
    )?;
    drop(conn);
    // URL/type 变了就是另一个上游目录：旧目录如果已经没有账号引用，别把模型行留在库里。
    if old_catalog_key.as_deref() != Some(new_catalog_key.as_str()) {
        let _ = crate::db::model::delete_orphan_catalogs(pool);
    }
    Ok(platform)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let mut conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let tx = conn.transaction().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    // 共享目录行登记在某个账号下；删账号前按每行自己的 catalog_key 改挂兄弟账号，
    // 不能只看平台当前 key——URL 改过之后旧目录行仍可能被其他账号使用。
    tx.execute(
        "UPDATE models SET platform_id = (
             SELECT p2.id FROM platforms p2
             WHERE p2.catalog_key = models.catalog_key AND p2.id != ?1
             LIMIT 1
         )
         WHERE platform_id = ?1 AND EXISTS (
             SELECT 1 FROM platforms p2
             WHERE p2.catalog_key = models.catalog_key AND p2.id != ?1
         )",
        [id],
    )?;
    tx.execute("DELETE FROM platforms WHERE id = ?1", [id])?;
    tx.commit().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    drop(conn);
    let _ = crate::db::model::delete_orphan_catalogs(pool);
    Ok(())
}
