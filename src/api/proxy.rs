use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::proxy::*;
use crate::models::model::Capability;
use std::collections::{HashMap, HashSet};

/// Compute capabilities for a proxy as intersection of all backend model capabilities
fn compute_caps_from_backends(
    backends: &[crate::models::route::Backend],
    model_map: &HashMap<String, crate::models::model::Model>,
) -> Vec<Capability> {
    let mut intersection: Option<HashSet<Capability>> = None;
    for b in backends {
        if let Some(model) = model_map.get(&b.model_id) {
            let caps: HashSet<Capability> = model.capabilities.iter().cloned().collect();
            intersection = Some(match intersection {
                None => caps,
                Some(prev) => prev.intersection(&caps).cloned().collect(),
            });
        }
    }
    intersection.map(|set| set.into_iter().collect()).unwrap_or_default()
}

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let result = web::block(move || -> AppResult<Vec<Proxy>> {
        let mut proxies = crate::db::proxy::list(&db)?;

        // For each proxy, find its route + backends + models, compute capabilities
        for proxy in proxies.iter_mut() {
            if let Ok(Some(route)) = crate::db::route::get_by_proxy(&db, &proxy.id) {
                let backends = crate::db::route::list_backends_by_route(&db, &route.id).unwrap_or_default();
                let mut model_map: HashMap<String, crate::models::model::Model> = HashMap::new();
                for b in &backends {
                    if !model_map.contains_key(&b.model_id) {
                        if let Ok(model) = crate::db::model::get(&db, &b.model_id) {
                            model_map.insert(b.model_id.clone(), model);
                        }
                    }
                }
                proxy.capabilities = compute_caps_from_backends(&backends, &model_map);
            }
        }

        Ok(proxies)
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(HttpResponse::Ok().json(result))
}

pub async fn get(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db_pool = db.into_inner();
    let db1 = db_pool.clone();
    let id2 = id.clone();
    let mut proxy = web::block(move || crate::db::proxy::get(&db1, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let db2 = db_pool.clone();
    let routes = web::block(move || crate::db::route::list_by_proxy(&db2, &id2))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    // Compute capabilities for this proxy
    if let Some(route) = routes.first() {
        let db3 = db_pool.clone();
        let route_id = route.id.clone();
        let backends = web::block(move || crate::db::route::list_backends_by_route(&db3, &route_id))
            .await.map_err(|e| AppError::Internal(e.to_string()))??;
        let mut model_map: HashMap<String, crate::models::model::Model> = HashMap::new();
        for b in &backends {
            let db4 = db_pool.clone();
            let mid = b.model_id.clone();
            if let Ok(model) = web::block(move || crate::db::model::get(&db4, &mid)).await.map_err(|e| AppError::Internal(e.to_string()))? {
                model_map.insert(b.model_id.clone(), model);
            }
        }
        proxy.capabilities = compute_caps_from_backends(&backends, &model_map);
    }

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
    web::block(move || crate::db::proxy::delete(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::NoContent().finish())
}
