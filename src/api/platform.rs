use actix_web::{web, HttpResponse};
use crate::cache::RouteCache;
use crate::config::SystemProxy;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::platform::*;
use std::sync::Arc;

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let platforms = web::block(move || crate::db::platform::list(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(platforms))
}

pub async fn list_presets() -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(PlatformPreset::all()))
}

pub async fn get(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let platform = web::block(move || crate::db::platform::get(&db, &id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(platform))
}

pub async fn create(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    body: web::Json<CreatePlatformRequest>,
) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let db = db.into_inner();
    let platform = web::block(move || crate::db::platform::create(&db, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::Created().json(platform))
}

pub async fn update(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<UpdatePlatformRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let platform = web::block(move || crate::db::platform::update(&db, &id, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::Ok().json(platform))
}

pub async fn delete(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::platform::delete(&db, &id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::NoContent().finish())
}

/// Fetch available models from a platform's remote API (e.g. GET /v1/models)
pub async fn fetch_remote_models(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let timeout_secs = config.read().defaults.test_connection_timeout_secs;

    let platform = web::block(move || crate::db::platform::get(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let outcome = probe(
        &build_client(timeout_secs)?,
        &platform.base_url,
        &platform.api_key,
        &platform.platform_type,
        &platform.custom_headers,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(HttpResponse::Ok().json(outcome_json(outcome)))
}

/// 把远端模型列表落库成 `models` 行：只补缺失的，绝不动已有的。
/// 价格、能力、显示名都是人工维护的；远端少一行通常只是 Key 的权限范围裁掉了它，
/// 并不代表模型消失，所以这里也绝不删除。
pub async fn sync_models(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let timeout_secs = config.read().defaults.test_connection_timeout_secs;

    let read_db = db.clone();
    let platform = web::block(move || crate::db::platform::get(&read_db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let outcome = match probe(
        &build_client(timeout_secs)?,
        &platform.base_url,
        &platform.api_key,
        &platform.platform_type,
        &platform.custom_headers,
    )
    .await
    {
        Ok(outcome) => outcome,
        // 与测试接口同一约定：上游问题作为数据返回，不抛 500
        Err(error) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": false, "error": error })));
        }
    };

    let remote_ids: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        outcome
            .models
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
            .filter(|mid| !mid.is_empty() && seen.insert(mid.to_string()))
            .map(str::to_string)
            .collect()
    };
    let total = remote_ids.len();

    let write_db = db.clone();
    let platform_id = platform.id.clone();
    let remote = remote_ids;
    let (created, existing) = web::block(move || crate::db::model::import_model_ids(&write_db, &platform_id, &remote))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let mut json = serde_json::json!({
        "ok": true,
        "created": created,
        "existing": existing,
        "total": total,
    });
    if let Some(note) = outcome.note {
        json["note"] = serde_json::json!(note);
    }
    Ok(HttpResponse::Ok().json(json))
}

/// 多选导入：只建勾选的那些，其余口径与同步接口完全一致（同一个助手函数）。
/// 远端列表由前端从 `GET /platforms/{id}/remote-models` 取，这里不重复探测。
pub async fn import_models(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    body: web::Json<crate::models::model::ImportModelsRequest>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let model_ids = body.into_inner().model_ids;
    if model_ids.iter().all(|id| id.trim().is_empty()) {
        return Err(AppError::BadRequest("至少选择一个模型".to_string()));
    }
    let db = db.into_inner();
    let (created, existing) = web::block(move || crate::db::model::import_model_ids(&db, &platform_id, &model_ids))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "created": created,
        "existing": existing,
        "requested": created + existing,
    })))
}

/// 批量编辑本平台已入库的模型：一次改多行的单价与能力。
/// 单价是读路由时 JOIN models 补进缓存的，改完必须刷缓存，否则之后的请求仍按旧价落库。
pub async fn batch_update_models(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<crate::models::model::BatchModelPatch>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let patch = body.into_inner();
    if !patch.has_change() {
        return Err(AppError::BadRequest("没有要修改的字段".to_string()));
    }
    let db = db.into_inner();
    let updated = web::block(move || crate::db::model::batch_patch(&db, &platform_id, &patch))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": true, "updated": updated })))
}

/// 一次定价导入的行数上限。CSV 是用户手写的，几百行的厂商价目表很常见；
/// 上限只用来挡住把整张无关大表粘进来的情况。
const PRICE_IMPORT_MAX_ROWS: usize = 5000;

/// 导出本平台的定价表：`model_id,input_price,output_price,credit_price`，空单元格就是未定价。
/// 导出的正是导入吃的格式——"下载 → 改数字 → 传回来"这条回路不需要用户猜列名。
/// 只导价格，不导显示名 / 能力 / 状态：那些列导入时不写，出现在文件里只会让人以为改了会生效。
pub async fn export_prices(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let read_db = db.into_inner();
    let models = web::block(move || {
        // 先确认平台存在：不查这一步，不存在的 id 会导出一份空 CSV 而不是 404。
        // 这里用 list 而不是 db::platform::get——后者把"查不到"落成 rusqlite 错误（500），
        // 而导出接口要的是 404。
        let platforms = crate::db::platform::list(&read_db)?;
        if !platforms.iter().any(|platform| platform.id == id) {
            return Err(AppError::NotFound("platform not found".to_string()));
        }
        crate::db::model::list_by_platform(&read_db, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let mut body = String::from("\u{feff}"); // BOM：没有它 Excel 会把中文列名读成乱码
    body.push_str("model_id,input_price,output_price,cache_read_price,cache_write_price,credit_price\n");
    for model in &models {
        let cells = [
            crate::api::stats::csv_text(&model.model_id),
            crate::api::stats::csv_num(model.input_price),
            crate::api::stats::csv_num(model.output_price),
            crate::api::stats::csv_num(model.cache_read_price),
            crate::api::stats::csv_num(model.cache_write_price),
            crate::api::stats::csv_num(model.credit_price),
        ];
        body.push_str(&cells.join(","));
        body.push('\n');
    }

    let name = format!("aigw-prices-{}.csv", chrono::Utc::now().format("%Y%m%d-%H%M"));
    Ok(HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header(("Content-Disposition", format!("attachment; filename=\"{name}\"")))
        .insert_header(("X-Export-Rows", models.len().to_string()))
        .body(body))
}

/// 定价 CSV 导入：只改本平台已有模型的单价，不建模型、不碰显示名 / 能力 / 状态 / 档位。
/// 未匹配的 ID 计入 `unmatched` 而不是整批报错——一份几百行的价目表里对上一两个是常态，
/// 整批拒绝会让这功能没法用；但数字必须回给前端，否则"没生效"和"生效了"看起来一模一样。
pub async fn import_prices(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<crate::models::model::ImportPricesRequest>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let rows = body.into_inner().prices;
    if rows.is_empty() {
        return Err(AppError::BadRequest("没有可导入的价格行".to_string()));
    }
    if rows.len() > PRICE_IMPORT_MAX_ROWS {
        return Err(AppError::BadRequest(format!(
            "一次最多导入 {} 行，请拆分文件",
            PRICE_IMPORT_MAX_ROWS
        )));
    }
    // 负价是纯输入错误，但它是能从 CSV 里进来的（`-` 开头的单元格、括号表示负数），
    // 而负成本会一路污染聚合，所以在门口拦掉整批。
    let negative = rows.iter().any(|row| {
        matches!(row.input_price, crate::models::model::NullableNumber::Set(v) if v < 0.0)
            || matches!(row.output_price, crate::models::model::NullableNumber::Set(v) if v < 0.0)
            || matches!(row.cache_read_price, crate::models::model::NullableNumber::Set(v) if v < 0.0)
            || matches!(row.cache_write_price, crate::models::model::NullableNumber::Set(v) if v < 0.0)
            || matches!(row.credit_price, crate::models::model::NullableNumber::Set(v) if v < 0.0)
    });
    if negative {
        return Err(AppError::BadRequest("价格不能小于 0".to_string()));
    }

    let db = db.into_inner();
    let result = web::block(move || crate::db::model::set_prices(&db, &platform_id, &rows))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    // 单价是读路由时 JOIN models 补进 RouteCache 的，不刷缓存之后的请求仍按旧价落 cost
    if result.written > 0 {
        cache.refresh();
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "written": result.written,
        "skipped": result.skipped,
        "unmatched": result.unmatched,
    })))
}

/// 保存前测试：用表单里的配置直接探测，不读库；platform_id 存在时只额外落一行健康历史。
/// Key 留空时若带了 platform_id，就用库里那份（只用于发请求，不回传）。
pub async fn test_config(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    hub: web::Data<crate::events::EventHub>,
    body: web::Json<PlatformTestRequest>,
) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let base_url = req.base_url.trim().to_string();
    if base_url.is_empty() {
        return Err(AppError::BadRequest("需要先填写 API 地址".to_string()));
    }
    let timeout_secs = config.read().defaults.test_connection_timeout_secs;
    let platform_id = req.platform_id.filter(|id| !id.trim().is_empty());

    let stored_key = match &platform_id {
        Some(id) => {
            let db = db.clone().into_inner();
            let id = id.clone();
            web::block(move || crate::db::platform::get(&db, &id))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?
                .map(|platform| platform.api_key)
                .unwrap_or_default()
        }
        None => String::new(),
    };
    let api_key = req
        .api_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .unwrap_or(stored_key);

    let start = std::time::Instant::now();
    let custom_headers = req.custom_headers.unwrap_or_else(|| serde_json::json!({}));
    let result = probe(&build_client(timeout_secs)?, &base_url, &api_key, &req.platform_type, &custom_headers).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let (ok, model_count, error) = match &result {
        Ok(outcome) => (true, outcome.models.len() as i64, None),
        Err(error) => (false, 0, Some(error.clone())),
    };

    // 健康历史是副作用：写失败只影响列表页的 CHECK 列，不该让测试本身报错
    if let Some(id) = &platform_id {
        let db = db.clone().into_inner();
        let row = id.clone();
        let err = error.clone();
        let _ = web::block(move || {
            crate::db::stats::record_platform_health(&db, &row, ok, latency_ms as i64, model_count, err.as_deref())
        })
        .await;
        // 另一扇开着的浏览器也该看到这次结果，否则它要等下一轮轮询才发现平台挂了
        hub.publish(crate::events::ConsoleEvent::PlatformCheck {
            platform_id: id.clone(),
            ok,
            latency_ms: Some(latency_ms),
            model_count: if ok { Some(model_count as usize) } else { None },
            error: error.clone().map(|e| e.chars().take(300).collect()),
        });
    }

    let resp = match result {
        Ok(outcome) => {
            let mut json = outcome_json(outcome);
            json["ok"] = serde_json::json!(true);
            json["latency_ms"] = serde_json::json!(latency_ms);
            json
        }
        Err(error) => serde_json::json!({
            "ok": false,
            "latency_ms": latency_ms,
            "model_count": 0,
            "models": [],
            "error": error,
        }),
    };
    Ok(HttpResponse::Ok().json(resp))
}

/// 探测结果：模型列表 + 一句可选说明（例如 Anthropic 没有列表接口）
struct ProbeOutcome {
    models: Vec<serde_json::Value>,
    note: Option<String>,
}

fn outcome_json(outcome: ProbeOutcome) -> serde_json::Value {
    let ProbeOutcome { models, note } = outcome;
    let mut json = serde_json::json!({ "models": models, "model_count": models.len() });
    if let Some(note) = note {
        json["note"] = serde_json::json!(note);
    }
    json
}

fn build_client(timeout_secs: u64) -> AppResult<reqwest::Client> {
    SystemProxy::detect()
        .apply_to_builder(reqwest::Client::builder().timeout(std::time::Duration::from_secs(timeout_secs)))
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// 所有启用积分预算的平台读数。平台列表/详情用它，不在 `Platform` 上塞运行态字段。
pub async fn credits(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let usage = web::block(move || crate::db::credit::usage_all(&db))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(usage))
}

/// 手动校准当前窗口用量。delta 是补记/冲减的积分，`spent` 下限为 0。
pub async fn adjust_credit(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    body: web::Json<crate::models::credit::CreditAdjustRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let note = req.note.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let db = db.into_inner();
    let (before, after) = web::block(move || crate::db::credit::adjust(&db, &id, req.delta, note.as_deref()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "before": before,
        "after": after,
    })))
}

/// 手动把当前窗口清零。用于上游额度被重置但本地还没到自然窗口起点。
pub async fn reset_credit(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let before = web::block(move || crate::db::credit::reset(&db, &id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": true, "before": before })))
}

/// 按平台类型探测：OpenAI 兼容走 GET /models，Anthropic 没有标准列表接口，
/// 改为请求 /v1/models 只确认"可达 + Key 有效"，不发消息也就消耗不了 token。
/// `custom_headers` 与代理转发同口径：鉴权头之后附加，因此同名自定义头会覆盖内置鉴权头。
async fn probe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    platform_type: &PlatformType,
    custom_headers: &serde_json::Value,
) -> Result<ProbeOutcome, String> {
    let base = base_url.trim_end_matches('/');
    let mut request = match platform_type {
        PlatformType::Anthropic => {
            let mut req = client
                .get(format!("{}/v1/models", base))
                .header("anthropic-version", "2023-06-01");
            if !api_key.is_empty() {
                req = req.header("x-api-key", api_key);
            }
            req
        }
        _ => {
            let mut req = client.get(format!("{}/models", base));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            req
        }
    };
    if let Some(map) = custom_headers.as_object() {
        for (name, value) in map {
            if let Some(vs) = value.as_str() {
                request = request.header(name.as_str(), vs);
            }
        }
    }

    let resp = request.send().await.map_err(|e| format!("连接失败：{}", e))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(format!("Key 被拒绝（HTTP {}）", status.as_u16()));
    }
    if matches!(platform_type, PlatformType::Anthropic) && status.as_u16() == 404 {
        // 端点在、Key 没被拒，只是这个平台不开放列表接口
        return Ok(ProbeOutcome {
            models: Vec::new(),
            note: Some("服务可达（HTTP 404），该类型没有模型列表接口".to_string()),
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "接口返回 {}：{}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应无法解析：{}", e))?;
    let models: Vec<serde_json::Value> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| serde_json::json!({
                    "id": m.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    "owned_by": m.get("owned_by").and_then(|v| v.as_str()).unwrap_or(""),
                }))
                .collect()
        })
        .unwrap_or_default();
    Ok(ProbeOutcome { models, note: None })
}

