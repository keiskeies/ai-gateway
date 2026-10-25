use crate::error::AppResult;
use crate::models::model::*;
use crate::db::DbPool;

pub fn list(pool: &DbPool) -> AppResult<Vec<Model>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, credit_price, capabilities, status FROM models ORDER BY display_name"
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
            credit_price: row.get(8)?,
            capabilities: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
            status: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or(ModelStatus::Active),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_by_platform(pool: &DbPool, platform_id: &str) -> AppResult<Vec<Model>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, credit_price, capabilities, status FROM models WHERE platform_id = ?1 ORDER BY display_name"
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
            credit_price: row.get(8)?,
            capabilities: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
            status: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or(ModelStatus::Active),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(pool: &DbPool, id: &str) -> AppResult<Model> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.query_row(
        "SELECT id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, credit_price, capabilities, status FROM models WHERE id = ?1",
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
                credit_price: row.get(8)?,
                capabilities: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
                status: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or(ModelStatus::Active),
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
        credit_price: req.credit_price,
        capabilities: req.capabilities.clone(),
        status: ModelStatus::Active,
    };

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, credit_price, capabilities, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            model.id, model.platform_id, model.model_id, model.display_name,
            model.max_tokens, model.context_window, model.input_price, model.output_price,
            model.credit_price,
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
            credit_price: req.credit_price,
            capabilities: req.capabilities.clone(),
            status: ModelStatus::Active,
        };
        conn.execute(
            "INSERT OR IGNORE INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, credit_price, capabilities, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                model.id, model.platform_id, model.model_id, model.display_name,
                model.max_tokens, model.context_window, model.input_price, model.output_price,
                model.credit_price,
                serde_json::to_string(&model.capabilities)?,
                serde_json::to_string(&model.status)?,
            ],
        )?;
        result.push(model);
    }
    Ok(result)
}

/// 按勾选把远端模型导入库里：去重、跳过库里已有的，剩下的用默认档位建。
/// 返回 `(新建, 已存在)`——"已存在"要单独报，否则一次导入 0 条看起来像失败。
/// 同步全部（`api::platform::sync_models`）与多选导入走同一个助手，
/// 两处对"已存在"的判定不可能分家。
pub fn import_model_ids(pool: &DbPool, platform_id: &str, model_ids: &[String]) -> AppResult<(usize, usize)> {
    let known: std::collections::HashSet<String> = list_by_platform(pool, platform_id)?
        .into_iter()
        .map(|model| model.model_id)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut missing = Vec::new();
    let mut existing = 0usize;
    for raw in model_ids {
        let model_id = raw.trim();
        if model_id.is_empty() || !seen.insert(model_id.to_string()) {
            continue;
        }
        if known.contains(model_id) {
            existing += 1;
        } else {
            missing.push(model_id.to_string());
        }
    }
    if !missing.is_empty() {
        batch_create(pool, missing.iter().map(|model_id| CreateModelRequest {
            platform_id: platform_id.to_string(),
            model_id: model_id.clone(),
            display_name: model_id.clone(),
            max_tokens: default_max_tokens(),
            context_window: default_context_window(),
            input_price: None,
            output_price: None,
            credit_price: None,
            capabilities: vec![],
        }).collect())?;
    }
    Ok((missing.len(), existing))
}

/// 三态写入：没提交这个字段就留原值，提交 `null` 落 NULL，提交数字落数字。
fn apply_nullable(field: crate::models::model::NullableNumber, current: Option<f64>) -> Option<f64> {
    match field {
        crate::models::model::NullableNumber::Keep => current,
        crate::models::model::NullableNumber::Clear => None,
        crate::models::model::NullableNumber::Set(value) => Some(value),
    }
}

pub fn update(pool: &DbPool, id: &str, req: &UpdateModelRequest) -> AppResult<Model> {
    let mut model = get(pool, id)?;
    if let Some(n) = &req.display_name { model.display_name = n.clone(); }
    if let Some(t) = req.max_tokens { model.max_tokens = t; }
    if let Some(c) = req.context_window { model.context_window = c; }
    model.input_price = apply_nullable(req.input_price, model.input_price);
    model.output_price = apply_nullable(req.output_price, model.output_price);
    model.credit_price = apply_nullable(req.credit_price, model.credit_price);
    if let Some(c) = &req.capabilities { model.capabilities = c.clone(); }
    if let Some(s) = &req.status { model.status = s.clone(); }

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE models SET display_name=?2, max_tokens=?3, context_window=?4, input_price=?5, output_price=?6, credit_price=?7, capabilities=?8, status=?9 WHERE id=?1",
        rusqlite::params![
            id, model.display_name, model.max_tokens, model.context_window,
            model.input_price, model.output_price, model.credit_price,
            serde_json::to_string(&model.capabilities)?,
            serde_json::to_string(&model.status)?,
        ],
    )?;
    Ok(model)
}

/// 批量改价 / 改能力：一条 UPDATE 覆盖整批，不逐行读改写——逐行 get + update 会把显示名、
/// 状态这些没打算动的字段一起写回去，批量越大越容易把别人的改动盖掉。
pub fn batch_patch(pool: &DbPool, platform_id: &str, patch: &BatchModelPatch) -> AppResult<usize> {
    let mut ids: Vec<String> = Vec::new();
    for raw in &patch.model_ids {
        let id = raw.trim();
        if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
    }
    if ids.is_empty() {
        return Err(crate::error::AppError::BadRequest("model_ids must not be empty".to_string()));
    }

    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // 先确认每一行都挂在这个平台下：混进别处的 id 会让一次编辑悄悄改掉别的平台的定价，
    // 而界面只会显示本平台那几行，改了什么根本看不见。
    let select = format!("SELECT id FROM models WHERE platform_id=?1 AND id IN ({})", placeholders);
    let mut owned: Vec<String> = Vec::new();
    {
        let mut stmt = conn.prepare(&select)?;
        let mut args: Vec<&dyn rusqlite::types::ToSql> = vec![&platform_id];
        for id in &ids {
            args.push(id);
        }
        for row in stmt.query_map(args.as_slice(), |row| row.get::<_, String>(0))? {
            owned.push(row?);
        }
    }
    if owned.len() != ids.len() {
        let missing = ids.iter().find(|id| !owned.iter().any(|owned| owned == *id))
            .cloned().unwrap_or_default();
        return Err(crate::error::AppError::BadRequest(format!(
            "{} is not a model of this platform", missing
        )));
    }

    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    match patch.input_price {
        NullableNumber::Keep => {}
        NullableNumber::Clear => sets.push("input_price=NULL".to_string()),
        NullableNumber::Set(v) => {
            sets.push("input_price=?".to_string());
            values.push(Box::new(v));
        }
    }
    match patch.output_price {
        NullableNumber::Keep => {}
        NullableNumber::Clear => sets.push("output_price=NULL".to_string()),
        NullableNumber::Set(v) => {
            sets.push("output_price=?".to_string());
            values.push(Box::new(v));
        }
    }
    match patch.credit_price {
        NullableNumber::Keep => {}
        NullableNumber::Clear => sets.push("credit_price=NULL".to_string()),
        NullableNumber::Set(v) => {
            sets.push("credit_price=?".to_string());
            values.push(Box::new(v));
        }
    }
    if let Some(capabilities) = &patch.capabilities {
        sets.push("capabilities=?".to_string());
        values.push(Box::new(serde_json::to_string(capabilities)?));
    }
    if sets.is_empty() {
        return Ok(0);
    }

    // 占位符顺序＝SET 的值 → platform_id → 各 id，values 必须按同一顺序压进去
    values.push(Box::new(platform_id.to_string()));
    for id in &ids {
        values.push(Box::new(id.clone()));
    }
    let args: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let sql = format!("UPDATE models SET {} WHERE platform_id=? AND id IN ({})", sets.join(", "), placeholders);
    let changed = conn.execute(sql.as_str(), args.as_slice())?;
    Ok(changed)
}

pub fn delete(pool: &DbPool, id: &str) -> AppResult<()> {    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM models WHERE id = ?1", [id])?;
    Ok(())
}

/// 定价导入的结果计数。`unmatched` 必须单独报：CSV 里拼错的模型 ID 如果静默跳过，
/// 用户会以为价格已经生效，而成本列只会一直显示 `—`。
#[derive(Debug, Default, PartialEq)]
pub struct PriceImportResult {
    /// 真正写入的行数（模型属于本平台且至少有一个字段要改）
    pub written: usize,
    /// 两个价格字段都是"不改"，没有可写内容
    pub skipped: usize,
    /// 本平台库里没有这个模型 ID——不新建，只报数
    pub unmatched: usize,
}

/// 按定价 CSV 逐行写单价：只碰本平台已有的模型行，只碰价格两列。
///
/// 每行一条 UPDATE，而不是 `batch_patch` 那种一条 `IN` ——批量编辑是"整批同一个值"，
/// CSV 是每行各自的值，一条 SQL 表达不了。SET 子句仍按三态拼：没提交的字段不进 SQL，
/// `NULL` 与"不改"就分得开。
///
/// 整个导入放在一个事务里：中途失败不会留下"一半模型有价、一半没有"的库，
/// 而成本估算是逐行读价的，半套价格比没有价格更难查。
pub fn set_prices(
    pool: &DbPool,
    platform_id: &str,
    rows: &[ModelPriceRow],
) -> AppResult<PriceImportResult> {
    // 同一个模型 ID 出现多次时合并而不是整行覆盖：CSV 里同一模型分两行写（一行填入价、
    // 一行填出价）是常见写法，"最后一行覆盖"会把前一行刚填好的那侧抹成"不改"。
    // 合并规则＝后写的非 Keep 字段赢，与"最后一行"的直觉一致，又不丢字段。
    let mut merged: Vec<ModelPriceRow> = Vec::new();
    let mut positions: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for row in rows {
        let model_id = row.model_id.trim();
        if model_id.is_empty() {
            continue;
        }
        match positions.get(model_id) {
            Some(index) => {
                let target = &mut merged[*index];
                if row.input_price != NullableNumber::Keep {
                    target.input_price = row.input_price;
                }
                if row.output_price != NullableNumber::Keep {
                    target.output_price = row.output_price;
                }
                if row.credit_price != NullableNumber::Keep {
                    target.credit_price = row.credit_price;
                }
            }
            None => {
                positions.insert(model_id, merged.len());
                merged.push(ModelPriceRow {
                    model_id: model_id.to_string(),
                    input_price: row.input_price,
                    output_price: row.output_price,
                    credit_price: row.credit_price,
                });
            }
        }
    }

    let mut conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let tx = conn.transaction().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut out = PriceImportResult::default();

    for row in &merged {
        let mut sets: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for (field, intent) in [
            ("input_price", row.input_price),
            ("output_price", row.output_price),
            ("credit_price", row.credit_price),
        ] {
            match intent {
                NullableNumber::Keep => {}
                NullableNumber::Clear => sets.push(format!("{field}=NULL")),
                NullableNumber::Set(value) => {
                    sets.push(format!("{field}=?"));
                    values.push(Box::new(value));
                }
            }
        }
        if sets.is_empty() {
            out.skipped += 1;
            continue;
        }
        values.push(Box::new(platform_id.to_string()));
        values.push(Box::new(row.model_id.trim().to_string()));
        let args: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let sql = format!("UPDATE models SET {} WHERE platform_id=? AND model_id=?", sets.join(", "));
        match tx.execute(sql.as_str(), args.as_slice())? {
            0 => out.unmatched += 1,
            _ => out.written += 1,
        }
    }

    tx.commit().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("model-test-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::init_pool(&path).expect("init test db");
        (pool, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    use crate::models::model::NullableNumber;
    use crate::models::platform::{CreatePlatformRequest, PlatformType};

    fn blank_update() -> UpdateModelRequest {
        UpdateModelRequest {
            display_name: None,
            max_tokens: None,
            context_window: None,
            input_price: NullableNumber::Keep,
            output_price: NullableNumber::Keep,
            credit_price: NullableNumber::Keep,
            capabilities: None,
            status: None,
        }
    }

    fn seed_platform(pool: &DbPool) -> String {
        crate::db::platform::create(pool, &CreatePlatformRequest {
            name: "plat".to_string(),
            platform_type: PlatformType::OpenAI,
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key: "sk-test".to_string(),
            organization: None,
            custom_headers: serde_json::Value::Null,
            credit_budget: None,
            credit_window: None,
        })
        .expect("create platform")
        .id
    }

    /// 价格字段的三种写法必须可区分：缺省＝不改、`null`＝清空、数字＝设值。
    /// 少了这层区分，"同步入库"补出来的行就没法退回"未定价"，成本口径会一直挂着旧价格。
    #[test]
    fn update_request_tells_absent_from_null() {        let absent: UpdateModelRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(absent.input_price, NullableNumber::Keep);
        assert_eq!(absent.output_price, NullableNumber::Keep);

        let cleared: UpdateModelRequest = serde_json::from_value(serde_json::json!({ "input_price": null })).unwrap();
        assert_eq!(cleared.input_price, NullableNumber::Clear);

        let set: UpdateModelRequest = serde_json::from_value(serde_json::json!({ "input_price": 0.5 })).unwrap();
        assert_eq!(set.input_price, NullableNumber::Set(0.5));

        let whole: UpdateModelRequest = serde_json::from_value(serde_json::json!({ "input_price": 3 })).unwrap();
        assert_eq!(whole.input_price, NullableNumber::Set(3.0));

        // 字符串不是价格：宁可比错也不静默当成"没改"
        let bad = serde_json::from_value::<UpdateModelRequest>(serde_json::json!({ "input_price": "free" }));
        assert!(bad.is_err());
    }

    #[test]
    fn price_roundtrip_respects_absent_and_null() {
        let (pool, path) = test_db();
        let platform_id = seed_platform(&pool);
        let created = create(&pool, &CreateModelRequest {
            platform_id,
            model_id: "m1".to_string(),
            display_name: "m1".to_string(),
            max_tokens: 4096,
            context_window: 8192,
            input_price: Some(1.0),
            output_price: Some(2.0),
            credit_price: None,
            capabilities: vec![],
        }).expect("create model");

        let mut req = blank_update();
        req.display_name = Some("改名后的模型".to_string());
        let renamed = update(&pool, &created.id, &req).unwrap();
        assert_eq!(renamed.display_name, "改名后的模型");
        assert_eq!(renamed.input_price, Some(1.0));
        assert_eq!(renamed.output_price, Some(2.0));

        req = blank_update();
        req.input_price = NullableNumber::Clear;
        let cleared = update(&pool, &created.id, &req).unwrap();
        assert_eq!(cleared.input_price, None);
        assert_eq!(cleared.output_price, Some(2.0));

        req = blank_update();
        req.input_price = NullableNumber::Set(0.5);
        assert_eq!(update(&pool, &created.id, &req).unwrap().input_price, Some(0.5));

        // 落库确认：读回来仍是那个值，而不是序列化对象自己骗自己
        assert_eq!(get(&pool, &created.id).unwrap().input_price, Some(0.5));

        cleanup_db(&path);
    }

    fn seed_other_platform(pool: &DbPool) -> String {
        crate::db::platform::create(pool, &CreatePlatformRequest {
            name: "plat-other".to_string(),
            platform_type: PlatformType::OpenAI,
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key: "sk-test".to_string(),
            organization: None,
            custom_headers: serde_json::Value::Null,
            credit_budget: None,
            credit_window: None,
        })
        .expect("create other platform")
        .id
    }

    fn seed_model(pool: &DbPool, platform_id: &str, model_id: &str, price: f64) -> String {
        create(pool, &CreateModelRequest {
            platform_id: platform_id.to_string(),
            model_id: model_id.to_string(),
            display_name: format!("显示名 {}", model_id),
            max_tokens: 2048,
            context_window: 4096,
            input_price: Some(price),
            output_price: Some(price),
            credit_price: None,
            capabilities: vec![Capability::Vision],
        })
        .expect("create model")
        .id
    }

    /// 批量编辑必须只动它被给的那几项：单价按三态走，显示名 / 档位 / 状态一字不改。
    #[test]
    fn batch_patch_changes_only_the_fields_it_was_given() {
        let (pool, path) = test_db();
        let mine = seed_platform(&pool);
        let ids = vec![seed_model(&pool, &mine, "m1", 9.0), seed_model(&pool, &mine, "m2", 9.0)];

        // 重复 id 与带空白的 id 都要吃掉：一次 UPDATE 只该改两行
        let patch = BatchModelPatch {
            model_ids: vec![ids[0].clone(), format!(" {} ", ids[0]), ids[1].clone()],
            input_price: NullableNumber::Set(0.8),
            output_price: NullableNumber::Clear,
            credit_price: NullableNumber::Keep,
            capabilities: Some(vec![Capability::Reasoning, Capability::Tool]),
        };
        assert_eq!(batch_patch(&pool, &mine, &patch).unwrap(), 2);
        for id in &ids {
            let row = get(&pool, id).unwrap();
            assert_eq!(row.input_price, Some(0.8));
            assert_eq!(row.output_price, None);
            assert_eq!(row.capabilities, vec![Capability::Reasoning, Capability::Tool]);
            assert_eq!(row.display_name, format!("显示名 {}", row.model_id));
            assert_eq!(row.max_tokens, 2048);
            assert_eq!(row.context_window, 4096);
            assert_eq!(row.status, ModelStatus::Active);
        }

        // 只给能力时两档单价原样留着——批量最常见的误伤就是"顺手把价格清了"
        let caps_only = BatchModelPatch {
            model_ids: ids.clone(),
            input_price: NullableNumber::Keep,
            output_price: NullableNumber::Keep,
            credit_price: NullableNumber::Keep,
            capabilities: Some(vec![]),
        };
        assert_eq!(batch_patch(&pool, &mine, &caps_only).unwrap(), 2);
        let after = get(&pool, &ids[0]).unwrap();
        assert_eq!(after.input_price, Some(0.8));
        assert_eq!(after.output_price, None);
        assert!(after.capabilities.is_empty());

        cleanup_db(&path);
    }

    /// 混进别处的 id 要在落库前挡掉：界面只显示本平台那几行，改到别的平台的定价根本看不见。
    #[test]
    fn batch_patch_refuses_foreign_and_empty_ids() {
        let (pool, path) = test_db();
        let mine = seed_platform(&pool);
        let other = seed_other_platform(&pool);
        let mine_id = seed_model(&pool, &mine, "m1", 9.0);
        let foreign_id = seed_model(&pool, &other, "m1", 1.0);

        let mixed = BatchModelPatch {
            model_ids: vec![mine_id.clone(), foreign_id.clone()],
            input_price: NullableNumber::Set(2.0),
            output_price: NullableNumber::Keep,
            credit_price: NullableNumber::Keep,
            capabilities: None,
        };
        let err = batch_patch(&pool, &mine, &mixed).unwrap_err();
        assert!(matches!(err, crate::error::AppError::BadRequest(_)));
        assert!(format!("{:?}", err).contains("not a model of this platform"));
        // 校验先于写入：连那一条合法的也没被改
        assert_eq!(get(&pool, &mine_id).unwrap().input_price, Some(9.0));
        assert_eq!(get(&pool, &foreign_id).unwrap().input_price, Some(1.0));

        let empty = BatchModelPatch {
            model_ids: vec!["  ".to_string()],
            input_price: NullableNumber::Set(2.0),
            output_price: NullableNumber::Keep,
            credit_price: NullableNumber::Keep,
            capabilities: None,
        };
        assert!(matches!(batch_patch(&pool, &mine, &empty).unwrap_err(), crate::error::AppError::BadRequest(_)));

        cleanup_db(&path);
    }

    /// 勾选导入必须：去重、忽略空白、跳过库里已有的，且绝不改已有行的人工数据。
    #[test]
    fn import_model_ids_creates_only_the_missing_ones() {
        let (pool, path) = test_db();
        let platform_id = seed_platform(&pool);
        create(&pool, &CreateModelRequest {
            platform_id: platform_id.clone(),
            model_id: "gpt-4o".to_string(),
            display_name: "旗舰模型".to_string(),
            max_tokens: 1024,
            context_window: 2048,
            input_price: Some(1.5),
            output_price: Some(3.0),
            credit_price: None,
            capabilities: vec![],
        }).expect("create model");

        let picked = vec![
            "gpt-4o".to_string(),        // 已存在
            " gpt-4o-mini ".to_string(), // 带空格
            "gpt-4o-mini".to_string(),   // 与上一行重复
            "".to_string(),              // 空串
            "o1".to_string(),
        ];
        let (created, existing) = import_model_ids(&pool, &platform_id, &picked).unwrap();
        assert_eq!((created, existing), (2, 1));

        let rows = list_by_platform(&pool, &platform_id).unwrap();
        assert_eq!(rows.len(), 3);
        let kept = rows.iter().find(|m| m.model_id == "gpt-4o").unwrap();
        assert_eq!(kept.display_name, "旗舰模型");
        assert_eq!(kept.input_price, Some(1.5));
        let fresh = rows.iter().find(|m| m.model_id == "gpt-4o-mini").unwrap();
        assert_eq!(fresh.display_name, "gpt-4o-mini");
        assert_eq!(fresh.max_tokens, default_max_tokens());
        assert_eq!(fresh.context_window, default_context_window());
        assert_eq!(fresh.input_price, None);

        // 再导一次同样的勾选：全是"已存在"，一条不建
        assert_eq!(import_model_ids(&pool, &platform_id, &picked).unwrap(), (0, 3));
        assert_eq!(list_by_platform(&pool, &platform_id).unwrap().len(), 3);

        cleanup_db(&path);
    }

    /// 定价 CSV 导入的四条底线：只碰价格两列、只碰本平台的行、未匹配的 ID 只报数不新建、
    /// 同一个 ID 出现多次按字段合并。少了任何一条，一次导入都可能悄悄改掉别的东西。
    #[test]
    fn set_prices_touches_only_prices_of_this_platform() {
        let (pool, path) = test_db();
        let mine = seed_platform(&pool);
        let other = seed_other_platform(&pool);
        seed_model(&pool, &mine, "m1", 9.0);
        seed_model(&pool, &mine, "m0", 7.0);
        seed_model(&pool, &mine, "m3", 5.0);
        let foreign = seed_model(&pool, &other, "m1", 1.0);

        let rows = vec![
            // 只设一侧，另一侧 Keep
            ModelPriceRow { model_id: "m0".into(), input_price: NullableNumber::Set(0.5), output_price: NullableNumber::Keep, credit_price: NullableNumber::Keep },
            // 库里没有这个 ID：报 unmatched，不顺手建一行
            ModelPriceRow { model_id: " m2 ".into(), input_price: NullableNumber::Set(2.0), output_price: NullableNumber::Set(4.0), credit_price: NullableNumber::Keep },
            // 同一个模型分两行写：合并后＝设入价 + 清出价，而不是后一行把入价抹回"不改"
            ModelPriceRow { model_id: "m1".into(), input_price: NullableNumber::Set(0.6), output_price: NullableNumber::Keep, credit_price: NullableNumber::Keep },
            ModelPriceRow { model_id: "m1".into(), input_price: NullableNumber::Keep, output_price: NullableNumber::Clear, credit_price: NullableNumber::Keep },
            // 两个字段都不改：不进 SQL
            ModelPriceRow { model_id: "m3".into(), input_price: NullableNumber::Keep, output_price: NullableNumber::Keep, credit_price: NullableNumber::Keep },
        ];
        let out = set_prices(&pool, &mine, &rows).unwrap();
        assert_eq!((out.written, out.unmatched, out.skipped), (2, 1, 1));

        let mine_rows = list_by_platform(&pool, &mine).unwrap();
        assert_eq!(mine_rows.len(), 3, "未匹配的 m2 没有被建出来");
        let m1 = mine_rows.iter().find(|m| m.model_id == "m1").unwrap();
        assert_eq!(m1.input_price, Some(0.6), "两行合并后入价仍在");
        assert_eq!(m1.output_price, None, "Clear 落 NULL，不是 0");
        assert_eq!(m1.display_name, "显示名 m1");
        assert_eq!((m1.max_tokens, m1.context_window), (2048, 4096));
        assert_eq!(m1.capabilities, vec![Capability::Vision]);
        let m0 = mine_rows.iter().find(|m| m.model_id == "m0").unwrap();
        assert_eq!(m0.input_price, Some(0.5));
        assert_eq!(m0.output_price, Some(7.0), "没提交的那侧保持原值");
        assert_eq!(mine_rows.iter().find(|m| m.model_id == "m3").unwrap().input_price, Some(5.0));

        // models 的唯一键是 (platform_id, model_id)，跨平台同 ID 是常态：那一行必须一字未动
        assert_eq!(get(&pool, &foreign).unwrap().input_price, Some(1.0));

        cleanup_db(&path);
    }
}
