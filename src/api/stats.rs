use std::sync::Arc;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::proxy::handler::ProxyState;

/// 概览页统计通用时间范围（RFC3339，UTC；[from, to) 半开区间）
#[derive(Clone, Default, Deserialize)]
pub struct RangeQuery {
    /// 起始时间（含）
    pub from: Option<String>,
    /// 结束时间（不含）
    pub to: Option<String>,
}

pub async fn overview(
    db: web::Data<DbPool>,
    state: web::Data<Arc<ProxyState>>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    // 并发连接来自内存中的 selector 计数器，无需触库
    let active_connections = state.selector.active_connections_total();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::overview(&db, active_connections, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn proxy_stats(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::proxy_stats(&db, &proxy_id, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn platform_stats(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<RangeQuery>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || {
        crate::db::stats::platform_stats(&db, &platform_id, q.from.as_deref(), q.to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn backend_stats(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let backend_id = path.into_inner();
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::backend_stats(&db, &backend_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

#[derive(Deserialize)]
pub struct ModelTimeSeriesQuery {
    /// 粒度: hour / day / month / year
    pub granularity: Option<String>,
    /// 周期数量（未指定 from/to 时生效）
    pub limit: Option<usize>,
    /// 起始时间（RFC3339，UTC），含
    pub from: Option<String>,
    /// 结束时间（RFC3339，UTC），不含
    pub to: Option<String>,
}

pub async fn model_timeseries(
    db: web::Data<DbPool>,
    query: web::Query<ModelTimeSeriesQuery>,
) -> AppResult<HttpResponse> {
    let granularity = query.granularity.as_deref().unwrap_or("day").to_string();
    let limit = query.limit.unwrap_or_else(|| match granularity.as_str() {
        "hour" => 24,
        "month" => 12,
        "year" => 5,
        _ => 30,
    });
    let from = query.from.clone();
    let to = query.to.clone();
    let db = db.into_inner();
    let stats = web::block(move || {
        crate::db::stats::model_timeseries(&db, &granularity, limit, from.as_deref(), to.as_deref())
    })
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn error_stats(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::error_stats(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn activity_heatmap(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::activity_heatmap(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn latency_percentiles(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::latency_percentiles(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 模型维度汇总（时间范围内每模型全量统计，概览页模型性能矩阵用）
pub async fn model_summaries(db: web::Data<DbPool>, query: web::Query<RangeQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let stats = web::block(move || crate::db::stats::model_summaries(&db, q.from.as_deref(), q.to.as_deref()))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

/// 请求明细分页查询参数
#[derive(Deserialize)]
pub struct RequestLogQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// 精确匹配后端模型 id
    pub model: Option<String>,
    /// success / fail，缺省 all
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// 请求明细列表（倒序分页）
pub async fn request_logs(db: web::Data<DbPool>, query: web::Query<RequestLogQuery>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let q = query.into_inner();
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);
    let result = web::block(move || crate::db::stats::list_requests(
        &db,
        q.from.as_deref(),
        q.to.as_deref(),
        q.model.as_deref(),
        q.status.as_deref(),
        page,
        page_size,
    ))
    .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

/// 清理悬挂的历史统计行（指向已删除 proxy / route / backend 的 request_stats 记录）
pub async fn cleanup_stats(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let result = web::block(move || crate::db::stats::cleanup_dangling(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}
