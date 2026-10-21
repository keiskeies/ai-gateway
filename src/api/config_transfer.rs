use actix_web::{web, HttpResponse};
use std::sync::Arc;

use crate::api::settings::{ConfigResponse, SharedAppConfig};
use crate::cache::RouteCache;
use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::config_transfer::{ConfigDocument, ImportMode, CONFIG_FORMAT, CONFIG_VERSION};

#[derive(Debug, serde::Deserialize)]
pub struct ExportQuery {
    /// `?secrets=1`：把上游平台密钥一起写进文件。默认不带。
    pub secrets: Option<String>,
}

fn truthy(raw: &Option<String>) -> bool {
    matches!(raw.as_deref(), Some("1") | Some("true") | Some("yes"))
}

/// 导出的文件名带日期，一堆 `ai-gateway-config.json` 里没人分得清哪份是昨天的。
fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%d").to_string()
}

fn attachment_name() -> String {
    format!("{}-{}.json", CONFIG_FORMAT, stamp())
}

pub async fn export(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    query: web::Query<ExportQuery>,
) -> AppResult<HttpResponse> {
    let include_secrets = truthy(&query.secrets);
    let db = db.into_inner();
    let mut doc = web::block(move || db::config_transfer::export(&db, include_secrets))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    // config.toml 快照只作参考：encrypt_key / admin_token 不在 ConfigResponse 里，
    // 所以这一段天然没有口令，导出的就是设置页能改的那些值。
    let cfg = config.read();
    doc.settings = serde_json::to_value(ConfigResponse::from(&*cfg)).ok();
    drop(cfg);

    let body = serde_json::to_string_pretty(&doc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let name = attachment_name();
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(("Content-Disposition", format!("attachment; filename=\"{name}\"")))
        .body(body))
}

#[derive(Debug, serde::Deserialize)]
pub struct ModeQuery {
    /// `merge`（默认，只补没有的）或 `replace`（六张配置表按文档重建）
    pub mode: Option<String>,
}

fn parse_mode(raw: &Option<String>) -> AppResult<ImportMode> {
    let normalized = raw.as_deref().map(str::trim).map(str::to_ascii_lowercase);
    match normalized.as_deref() {
        None | Some("") | Some("merge") => Ok(ImportMode::Merge),
        Some("replace") => Ok(ImportMode::Replace),
        Some(other) => Err(AppError::BadRequest(format!(
            "Unknown import mode {other}; use merge or replace"
        ))),
    }
}

/// 一个数字文档就是"没有 version 字段"的那一类，先按形状挡掉再谈内容
fn shape_issue(doc: &ConfigDocument) -> Option<AppError> {
    if doc.format.as_deref() == Some(CONFIG_FORMAT) && doc.version == Some(CONFIG_VERSION) {
        return None;
    }
    let errors = doc.check();
    let detail = errors
        .iter()
        .map(|i| format!("{} {:?}", i.code, i.args))
        .collect::<Vec<_>>()
        .join(" / ");
    Some(AppError::BadRequest(format!(
        "Not an AI Gateway config document: {}",
        if detail.is_empty() { "unknown shape".to_string() } else { detail }
    )))
}

/// 预演：只读，返回"会新增/跳过/删除什么"。导入必须先看这一步，
/// 覆盖档会删掉本机配置，不能靠一个确认对话框兜住看不见的后果。
pub async fn plan(
    db: web::Data<DbPool>,
    query: web::Query<ModeQuery>,
    body: web::Json<ConfigDocument>,
) -> AppResult<HttpResponse> {
    let mode = parse_mode(&query.mode)?;
    let doc = body.into_inner();
    if let Some(err) = shape_issue(&doc) {
        return Err(err);
    }
    let db = db.into_inner();
    let plan = web::block(move || db::config_transfer::plan_pool(&db, &doc, mode))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(plan))
}

/// 真正写入。服务端会再跑一次同一份计划并拒绝不合格文档——
/// 预演和落库之间可能有人改过库，也可能有人跳过预演直接 POST。
pub async fn apply(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    query: web::Query<ModeQuery>,
    body: web::Json<ConfigDocument>,
) -> AppResult<HttpResponse> {
    let mode = parse_mode(&query.mode)?;
    let doc = body.into_inner();
    if let Some(err) = shape_issue(&doc) {
        return Err(err);
    }
    let db = db.into_inner();
    let result = web::block(move || db::config_transfer::apply_pool(&db, &doc, mode))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    // 路由与 Key 都换了：不刷缓存的话，导入完的虚拟模型还要等一轮 TTL 才生效
    cache.refresh();
    cache.refresh_api_keys();
    Ok(HttpResponse::Ok().json(result))
}
