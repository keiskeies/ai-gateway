use actix_web::{web, HttpResponse};
use serde::Deserialize;
use crate::db::DbPool;
use crate::error::AppResult;

pub async fn overview(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::overview(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn proxy_stats(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::proxy_stats(&db, &proxy_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn platform_stats(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::platform_stats(&db, &platform_id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}

#[derive(Deserialize)]
pub struct ModelTimeSeriesQuery {
    /// 粒度: day / month / year
    pub granularity: Option<String>,
    /// 周期数量
    pub limit: Option<usize>,
}

pub async fn model_timeseries(
    db: web::Data<DbPool>,
    query: web::Query<ModelTimeSeriesQuery>,
) -> AppResult<HttpResponse> {
    let granularity = query.granularity.as_deref().unwrap_or("day").to_string();
    let limit = query.limit.unwrap_or_else(|| match granularity.as_str() {
        "month" => 12,
        "year" => 5,
        _ => 30,
    });
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::model_timeseries(&db, &granularity, limit))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}
