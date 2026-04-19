use actix_web::{web, HttpResponse};
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
