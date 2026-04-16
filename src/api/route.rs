use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::route::*;

macro_rules! db_block {
    ($db:expr, $f:expr) => {
        web::block(move || $f).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??
    };
}

pub async fn list(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let routes = db_block!(db, crate::db::route::list_by_proxy(&db, &proxy_id));
    Ok(HttpResponse::Ok().json(routes))
}

pub async fn create(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<CreateRouteRequest>) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let route = db_block!(db, crate::db::route::create(&db, &proxy_id, &req));
    Ok(HttpResponse::Created().json(route))
}

pub async fn update(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<UpdateRouteRequest>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let route = db_block!(db, crate::db::route::update(&db, &id, &req));
    Ok(HttpResponse::Ok().json(route))
}

pub async fn delete(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    db_block!(db, crate::db::route::delete(&db, &id));
    Ok(HttpResponse::NoContent().finish())
}

pub async fn list_backends(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let route_id = path.into_inner();
    let db = db.into_inner();
    let backends = db_block!(db, crate::db::route::list_backends_by_route(&db, &route_id));
    Ok(HttpResponse::Ok().json(backends))
}

pub async fn add_backend(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<CreateBackendRequest>) -> AppResult<HttpResponse> {
    let route_id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let backend = db_block!(db, crate::db::route::add_backend(&db, &route_id, &req));
    Ok(HttpResponse::Created().json(backend))
}

pub async fn update_backend(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<UpdateBackendRequest>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let backend = db_block!(db, crate::db::route::update_backend(&db, &id, &req));
    Ok(HttpResponse::Ok().json(backend))
}

pub async fn delete_backend(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    db_block!(db, crate::db::route::delete_backend(&db, &id));
    Ok(HttpResponse::NoContent().finish())
}
