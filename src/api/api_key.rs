use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::api_key::*;

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let keys = web::block(move || crate::db::api_key::list(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(keys))
}

pub async fn create(db: web::Data<DbPool>, body: web::Json<CreateApiKeyRequest>) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let db = db.into_inner();
    let api_key = web::block(move || crate::db::api_key::create(&db, &req))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Created().json(api_key))
}

pub async fn delete(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::api_key::delete(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::NoContent().finish())
}
