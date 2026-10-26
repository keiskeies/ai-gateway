use crate::error::AppResult;
use crate::models::api_key::NullableString;
use crate::models::model::NullableNumber;
use crate::models::platform::*;
use crate::db::DbPool;

const PLATFORM_COLS: &str = "id, name, type, base_url, api_key, organization, custom_headers, status, \
    rate_limit, credit_budget, credit_window, catalog_key, created_at, updated_at";

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
    let credit_window = validate_credit_fields(req.credit_budget, req.credit_window.as_deref())
        .map_err(crate::error::AppError::BadRequest)?;
    let catalog_key = crate::catalog::catalog_key(&req.platform_type, &req.base_url);
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
        catalog_key: Some(catalog_key.clone()),
        created_at: now.clone(),
        updated_at: now,
    };

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO platforms (id, name, type, base_url, api_key, organization, custom_headers, status, rate_limit, credit_budget, credit_window, catalog_key, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
        ],
    )?;
    Ok(platform)
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
    let window = validate_credit_fields(budget, window.as_deref())
        .map_err(crate::error::AppError::BadRequest)?;
    platform.credit_budget = budget;
    platform.credit_window = window;
    let old_catalog_key = platform.catalog_key.clone();
    let new_catalog_key = crate::catalog::catalog_key(&platform.platform_type, &platform.base_url);
    platform.catalog_key = Some(new_catalog_key.clone());
    platform.updated_at = chrono::Utc::now().to_rfc3339();

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platforms SET name=?2, type=?3, base_url=?4, api_key=?5, organization=?6, custom_headers=?7, status=?8, credit_budget=?9, credit_window=?10, catalog_key=?11, updated_at=?12 WHERE id=?1",
        rusqlite::params![
            id, platform.name, serde_json::to_string(&platform.platform_type)?,
            platform.base_url, platform.api_key, platform.organization,
            serde_json::to_string(&platform.custom_headers)?,
            serde_json::to_string(&platform.status)?,
            platform.credit_budget, platform.credit_window, platform.catalog_key, platform.updated_at,
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
