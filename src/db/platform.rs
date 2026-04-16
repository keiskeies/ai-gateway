use crate::error::AppResult;
use crate::models::platform::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<Platform>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, name, type, base_url, api_key, organization, custom_headers, status, rate_limit, created_at, updated_at FROM platforms ORDER BY created_at"
    )?;
    let rows = stmt.query_map([], |row| {
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
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Platform> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row(
        "SELECT id, name, type, base_url, api_key, organization, custom_headers, status, rate_limit, created_at, updated_at FROM platforms WHERE id = ?1",
        [id],
        |row| {
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
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    ).map_err(Into::into)
}

pub fn create(pool: &DbPool, req: &CreatePlatformRequest) -> AppResult<Platform> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
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
        created_at: now.clone(),
        updated_at: now,
    };

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO platforms (id, name, type, base_url, api_key, organization, custom_headers, status, rate_limit, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
    platform.updated_at = chrono::Utc::now().to_rfc3339();

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platforms SET name=?2, type=?3, base_url=?4, api_key=?5, organization=?6, custom_headers=?7, status=?8, updated_at=?9 WHERE id=?1",
        rusqlite::params![
            id, platform.name, serde_json::to_string(&platform.platform_type)?,
            platform.base_url, platform.api_key, platform.organization,
            serde_json::to_string(&platform.custom_headers)?,
            serde_json::to_string(&platform.status)?, platform.updated_at,
        ],
    )?;
    Ok(platform)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM platforms WHERE id = ?1", [id])?;
    Ok(())
}
