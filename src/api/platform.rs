use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::platform::*;

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

pub async fn create(db: web::Data<DbPool>, body: web::Json<CreatePlatformRequest>) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let db = db.into_inner();
    let platform = web::block(move || crate::db::platform::create(&db, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Created().json(platform))
}

pub async fn update(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<UpdatePlatformRequest>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let platform = web::block(move || crate::db::platform::update(&db, &id, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(platform))
}

pub async fn delete(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::platform::delete(&db, &id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::NoContent().finish())
}
