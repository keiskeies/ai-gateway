use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::model::*;

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let models = web::block(move || crate::db::model::list(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(models))
}

pub async fn get(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let model = web::block(move || crate::db::model::get(&db, &id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(model))
}

pub async fn create(db: web::Data<DbPool>, body: web::Json<CreateModelRequest>) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let db = db.into_inner();
    let model = web::block(move || crate::db::model::create(&db, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Created().json(model))
}

pub async fn update(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<UpdateModelRequest>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let model = web::block(move || crate::db::model::update(&db, &id, &req))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(model))
}

pub async fn delete(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::model::delete(&db, &id))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::NoContent().finish())
}
