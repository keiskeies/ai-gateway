use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
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

/// Fetch available models from a platform's remote API (e.g. GET /v1/models)
pub async fn fetch_remote_models(
    db: web::Data<DbPool>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    let timeout_secs = config.read().defaults.test_connection_timeout_secs;

    let platform = web::block(move || crate::db::platform::get(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let base_url = platform.base_url.trim_end_matches('/').to_string();
    let api_key = platform.api_key.clone();
    let platform_type = platform.platform_type.clone();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let result = match platform_type {
        PlatformType::Anthropic => {
            // Anthropic doesn't have a standard models list API, return empty
            serde_json::json!({ "models": [] })
        }
        _ => {
            // OpenAI compatible: GET /v1/models
            let url = format!("{}/models", base_url);
            let mut req = client.get(&url);
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            let resp = req.send().await.map_err(|e| AppError::Internal(format!("Failed to fetch models: {}", e)))?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Internal(format!("API returned {}: {}", status, body.chars().take(200).collect::<String>())));
            }
            let body: serde_json::Value = resp.json().await.map_err(|e| AppError::Internal(format!("Failed to parse response: {}", e)))?;
            // Extract model ids from OpenAI-format response
            let models: Vec<serde_json::Value> = body.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().map(|m| serde_json::json!({
                    "id": m.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    "owned_by": m.get("owned_by").and_then(|v| v.as_str()).unwrap_or(""),
                })).collect())
                .unwrap_or_default();
            serde_json::json!({ "models": models })
        }
    };

    Ok(HttpResponse::Ok().json(result))
}
