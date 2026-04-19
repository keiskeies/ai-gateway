use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::model::*;

pub async fn test_connection(db: web::Data<DbPool>, config: web::Data<crate::api::settings::SharedAppConfig>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    
    // Get configured test timeout
    let timeout_secs = config.read().defaults.test_connection_timeout_secs;
    
    // Get model and its platform info
    let (model, platform) = web::block(move || -> AppResult<(crate::models::model::Model, crate::models::platform::Platform)> {
        let model = crate::db::model::get(&db, &id)?;
        let platform = crate::db::platform::get(&db, &model.platform_id)?;
        Ok((model, platform))
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;

    let start = std::time::Instant::now();
    
    // Build test request based on platform type
    let base_url = platform.base_url.trim_end_matches('/').to_string();
    let api_key = platform.api_key.clone();
    let model_id = model.model_id.clone();
    let platform_type = platform.platform_type.clone();
    
    let result = match platform_type {
        crate::models::platform::PlatformType::Anthropic => {
            // Anthropic: POST /v1/messages with minimal payload
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            
            let url = format!("{}/v1/messages", base_url);
            let mut req = client.post(&url)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": model_id,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}]
                }));
            
            if !api_key.is_empty() {
                req = req.header("x-api-key", &api_key);
            }
            
            req.send().await
        }
        _ => {
            // OpenAI compatible: GET /v1/models or minimal chat completion
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            
            let url = format!("{}/chat/completions", base_url);
            let mut req = client.post(&url)
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": model_id,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}]
                }));
            
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            
            req.send().await
        }
    };

    let elapsed = start.elapsed();
    let latency_ms = elapsed.as_millis() as u64;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let is_success = resp.status().is_success();
            // Read the response body for error details
            let body_text = resp.text().await.unwrap_or_default();
            
            if is_success {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "status": status,
                    "latency_ms": latency_ms,
                    "message": "Connection successful"
                })))
            } else {
                // Try to extract error message from response
                let error_msg = serde_json::from_str::<serde_json::Value>(&body_text)
                    .ok()
                    .and_then(|v| v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str().map(String::from)))
                    .unwrap_or_else(|| body_text.chars().take(200).collect());
                
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "success": false,
                    "status": status,
                    "latency_ms": latency_ms,
                    "message": format!("API returned {}: {}", status, error_msg)
                })))
            }
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                format!("Connection timed out ({}s)", timeout_secs)
            } else if e.is_connect() {
                format!("Cannot connect to server: {}", e)
            } else {
                format!("Connection error: {}", e)
            };
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": false,
                "status": 0,
                "latency_ms": latency_ms,
                "message": msg
            })))
        }
    }
}

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
