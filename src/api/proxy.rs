use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::proxy::*;
use std::sync::Arc;
use crate::proxy::handler::ProxyState;

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let proxies = web::block(move || crate::db::proxy::list(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(proxies))
}

pub async fn get(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let id2 = id.clone();
    let db2 = db.clone();
    let proxy = web::block(move || crate::db::proxy::get(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let routes = web::block(move || crate::db::route::list_by_proxy(&db2, &id2))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "proxy": proxy,
        "routes": routes,
    })))
}

pub async fn create(db: web::Data<DbPool>, body: web::Json<CreateProxyRequest>) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    let db = db.into_inner();
    let proxy = web::block(move || crate::db::proxy::create(&db, &req))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Created().json(proxy))
}

pub async fn update(db: web::Data<DbPool>, path: web::Path<String>, body: web::Json<UpdateProxyRequest>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let proxy = web::block(move || crate::db::proxy::update(&db, &id, &req))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(proxy))
}

pub async fn delete(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || {
        let _ = crate::db::proxy::update_status(&db, &id, &ProxyStatus::Stopped);
        crate::db::proxy::delete(&db, &id)
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn start(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    _state: web::Data<Arc<ProxyState>>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let proxy: crate::models::proxy::Proxy = web::block(move || -> AppResult<crate::models::proxy::Proxy> {
        let mut proxy = crate::db::proxy::get(&db, &id)?;
        if proxy.status == ProxyStatus::Running {
            return Err(AppError::BadRequest("Proxy is already running".to_string()));
        }
        crate::db::proxy::update_status(&db, &id, &ProxyStatus::Running)?;
        proxy.status = ProxyStatus::Running;
        Ok(proxy)
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;

    tracing::info!("Proxy '{}' started on port {}", proxy.name, proxy.listen_port);
    Ok(HttpResponse::Ok().json(proxy))
}

pub async fn stop(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let proxy: crate::models::proxy::Proxy = web::block(move || -> AppResult<crate::models::proxy::Proxy> {
        let mut proxy = crate::db::proxy::get(&db, &id)?;
        crate::db::proxy::update_status(&db, &id, &ProxyStatus::Stopped)?;
        proxy.status = ProxyStatus::Stopped;
        tracing::info!("Proxy '{}' stopped", proxy.name);
        Ok(proxy)
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(proxy))
}
