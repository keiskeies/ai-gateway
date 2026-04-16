use crate::error::AppResult;
use crate::models::model::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<Model>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status FROM models ORDER BY display_name"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Model {
            id: row.get(0)?,
            platform_id: row.get(1)?,
            model_id: row.get(2)?,
            display_name: row.get(3)?,
            max_tokens: row.get(4)?,
            context_window: row.get(5)?,
            input_price: row.get(6)?,
            output_price: row.get(7)?,
            capabilities: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
            status: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or(ModelStatus::Active),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_by_platform(pool: &DbPool, platform_id: &str) -> AppResult<Vec<Model>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status FROM models WHERE platform_id = ?1 ORDER BY display_name"
    )?;
    let rows = stmt.query_map([platform_id], |row| {
        Ok(Model {
            id: row.get(0)?,
            platform_id: row.get(1)?,
            model_id: row.get(2)?,
            display_name: row.get(3)?,
            max_tokens: row.get(4)?,
            context_window: row.get(5)?,
            input_price: row.get(6)?,
            output_price: row.get(7)?,
            capabilities: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
            status: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or(ModelStatus::Active),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Model> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status FROM models WHERE id = ?1",
        [id],
        |row| {
            Ok(Model {
                id: row.get(0)?,
                platform_id: row.get(1)?,
                model_id: row.get(2)?,
                display_name: row.get(3)?,
                max_tokens: row.get(4)?,
                context_window: row.get(5)?,
                input_price: row.get(6)?,
                output_price: row.get(7)?,
                capabilities: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                status: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or(ModelStatus::Active),
            })
        },
    ).map_err(Into::into)
}

pub fn create(pool: &DbPool, req: &CreateModelRequest) -> AppResult<Model> {
    let id = uuid::Uuid::new_v4().to_string();
    let model = Model {
        id,
        platform_id: req.platform_id.clone(),
        model_id: req.model_id.clone(),
        display_name: req.display_name.clone(),
        max_tokens: req.max_tokens,
        context_window: req.context_window,
        input_price: req.input_price,
        output_price: req.output_price,
        capabilities: req.capabilities.clone(),
        status: ModelStatus::Active,
    };

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            model.id, model.platform_id, model.model_id, model.display_name,
            model.max_tokens, model.context_window, model.input_price, model.output_price,
            serde_json::to_string(&model.capabilities)?,
            serde_json::to_string(&model.status)?,
        ],
    )?;
    Ok(model)
}

pub fn batch_create(pool: &DbPool, models: Vec<CreateModelRequest>) -> AppResult<Vec<Model>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut result = Vec::new();
    for req in models {
        let id = uuid::Uuid::new_v4().to_string();
        let model = Model {
            id,
            platform_id: req.platform_id.clone(),
            model_id: req.model_id.clone(),
            display_name: req.display_name.clone(),
            max_tokens: req.max_tokens,
            context_window: req.context_window,
            input_price: req.input_price,
            output_price: req.output_price,
            capabilities: req.capabilities.clone(),
            status: ModelStatus::Active,
        };
        conn.execute(
            "INSERT OR IGNORE INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                model.id, model.platform_id, model.model_id, model.display_name,
                model.max_tokens, model.context_window, model.input_price, model.output_price,
                serde_json::to_string(&model.capabilities)?,
                serde_json::to_string(&model.status)?,
            ],
        )?;
        result.push(model);
    }
    Ok(result)
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateModelRequest) -> AppResult<Model> {
    let mut model = get(pool, id)?;
    if let Some(n) = &req.display_name { model.display_name = n.clone(); }
    if let Some(t) = req.max_tokens { model.max_tokens = t; }
    if let Some(c) = req.context_window { model.context_window = c; }
    if let Some(p) = req.input_price { model.input_price = Some(p); }
    if let Some(p) = req.output_price { model.output_price = Some(p); }
    if let Some(c) = &req.capabilities { model.capabilities = c.clone(); }
    if let Some(s) = &req.status { model.status = s.clone(); }

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE models SET display_name=?2, max_tokens=?3, context_window=?4, input_price=?5, output_price=?6, capabilities=?7, status=?8 WHERE id=?1",
        rusqlite::params![
            id, model.display_name, model.max_tokens, model.context_window,
            model.input_price, model.output_price,
            serde_json::to_string(&model.capabilities)?,
            serde_json::to_string(&model.status)?,
        ],
    )?;
    Ok(model)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM models WHERE id = ?1", [id])?;
    Ok(())
}
