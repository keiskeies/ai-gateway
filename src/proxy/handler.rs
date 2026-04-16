use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Instant;

use crate::db::DbPool;
use crate::error::AppError;
use crate::lb::BackendSelector;
use crate::models::proxy::{Protocol, ProxyStatus};
use crate::models::route::ErrorType;
use crate::protocol::{openai, anthropic, types::UnifiedRequest};
use crate::config::AppConfig;

pub struct ProxyState {
    pub db: DbPool,
    pub config: AppConfig,
    pub selector: Arc<BackendSelector>,
    pub http_client: reqwest::Client,
}

fn find_running_proxy(db: &DbPool) -> Result<crate::models::proxy::Proxy, AppError> {
    let proxies = crate::db::proxy::list(db)?;
    proxies.iter().find(|p| p.status == ProxyStatus::Running)
        .or_else(|| proxies.first()).cloned()
        .ok_or_else(|| AppError::NotFound("No proxy found".to_string()))
}

fn extract_auth(req: &HttpRequest) -> Option<String> {
    req.headers().get("Authorization").and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).to_string())
        .or_else(|| req.headers().get("x-api-key").and_then(|v| v.to_str().ok()).map(String::from))
}

fn validate_auth(proxy: &crate::models::proxy::Proxy, auth: &Option<String>) -> Result<(), AppError> {
    if let Some(ref token) = proxy.auth_token {
        if auth.as_ref().map(|s| s.as_str()) != Some(token.as_str()) {
            return Err(AppError::BadRequest("Invalid auth token".to_string()));
        }
    }
    Ok(())
}

pub async fn openai_chat_completions(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let db = state.db.clone();
    let proxy = web::block(move || find_running_proxy(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    validate_auth(&proxy, &extract_auth(&req))?;
    let unified = openai::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, &proxy, unified, Protocol::OpenAI).await
}

pub async fn anthropic_messages(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let db = state.db.clone();
    let proxy = web::block(move || find_running_proxy(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    validate_auth(&proxy, &extract_auth(&req))?;
    let unified = anthropic::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, &proxy, unified, Protocol::Anthropic).await
}

pub async fn openai_list_models(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let db = state.db.clone();
    let proxy = web::block(move || find_running_proxy(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    validate_auth(&proxy, &extract_auth(&req))?;

    let db2 = state.db.clone();
    let proxy_id = proxy.id.clone();
    let routes = web::block(move || crate::db::route::list_by_proxy(&db2, &proxy_id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let models: Vec<String> = routes.iter().map(|r| r.virtual_model.clone()).collect();
    Ok(HttpResponse::Ok().json(openai::models_list(&models)))
}

async fn handle_request(
    state: &Arc<ProxyState>, proxy: &crate::models::proxy::Proxy,
    unified: UnifiedRequest, response_protocol: Protocol,
) -> Result<HttpResponse, AppError> {
    let virtual_model = unified.model.clone();
    let is_stream = unified.stream.unwrap_or(false);

    let db = state.db.clone();
    let proxy_id = proxy.id.clone();
    let vm = virtual_model.clone();
    let route = web::block(move || crate::db::route::get_by_virtual_model(&db, &proxy_id, &vm))
        .await.map_err(|e| AppError::Internal(e.to_string()))??
        .ok_or_else(|| AppError::NotFound(format!("Model '{}' not found", virtual_model)))?;

    let max_retries = route.retry_policy.max_retries;
    let mut last_error: Option<String> = None;

    for attempt in 0..=max_retries {
        let backend = state.selector.select(&route.id, &route.backends, &route.lb_strategy)
            .ok_or_else(|| AppError::Internal("No available backend".to_string()))?;

        let db = state.db.clone();
        let platform_id = backend.platform_id.clone();
        let platform = web::block(move || crate::db::platform::get(&db, &platform_id))
            .await.map_err(|e| AppError::Internal(e.to_string()))??;

        let db = state.db.clone();
        let model_id = backend.model_id.clone();
        let model = web::block(move || crate::db::model::get(&db, &model_id))
            .await.map_err(|e| AppError::Internal(e.to_string()))??;

        state.selector.inc_connection(&route.id, &backend.id);
        let start = Instant::now();

        let (forward_body, forward_url) = build_forward_request(&unified, &platform, &model.model_id);

        let mut req_builder = state.http_client.post(&forward_url)
            .timeout(std::time::Duration::from_secs(state.config.defaults.request_timeout_secs))
            .header("Content-Type", "application/json");

        if !platform.api_key.is_empty() {
            match platform.platform_type {
                crate::models::platform::PlatformType::Anthropic => {
                    req_builder = req_builder.header("x-api-key", &platform.api_key).header("anthropic-version", "2023-06-01");
                }
                _ => { req_builder = req_builder.header("Authorization", format!("Bearer {}", platform.api_key)); }
            }
        }
        if let Some(headers) = platform.custom_headers.as_object() {
            for (k, v) in headers { if let Some(vs) = v.as_str() { req_builder = req_builder.header(k.as_str(), vs); } }
        }

        let result = req_builder.json(&forward_body).send().await;
        state.selector.dec_connection(&route.id, &backend.id);

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if is_stream { return Ok(handle_stream(resp).await); }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let raw: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    let ur = match platform.platform_type {
                        crate::models::platform::PlatformType::Anthropic => anthropic::parse_response(raw).map_err(AppError::Internal)?,
                        _ => openai::parse_response(raw).map_err(AppError::Internal)?,
                    };
                    let cr = match response_protocol { Protocol::Anthropic => anthropic::to_response(&ur), _ => openai::to_response(&ur) };
                    return Ok(HttpResponse::Ok().json(cr));
                } else {
                    let eb = resp.text().await.unwrap_or_default();
                    last_error = Some(format!("{}: {}", status, eb));
                    if should_retry(&classify_error(status.as_u16()), &route.retry_policy.retry_on_error) && attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                last_error = Some(format!("Connection: {}", e));
                if should_retry(&ErrorType::ConnectionError, &route.retry_policy.retry_on_error) && attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                    continue;
                }
            }
        }
    }
    Err(AppError::Internal(format!("All retries: {}", last_error.unwrap_or_default())))
}

fn build_forward_request(unified: &UnifiedRequest, platform: &crate::models::platform::Platform, target_model: &str) -> (serde_json::Value, String) {
    match platform.platform_type {
        crate::models::platform::PlatformType::Anthropic => {
            (anthropic::to_request(unified, target_model), format!("{}/v1/messages", platform.base_url.trim_end_matches('/')))
        }
        _ => {
            (openai::to_request(unified, target_model), format!("{}/chat/completions", platform.base_url.trim_end_matches('/')))
        }
    }
}

async fn handle_stream(resp: reqwest::Response) -> HttpResponse {
    let s = resp.status();
    if !s.is_success() { return HttpResponse::build(actix_web::http::StatusCode::from_u16(s.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(resp.text().await.unwrap_or_default()); }
    HttpResponse::Ok().content_type("text/event-stream").insert_header(("Cache-Control","no-cache")).insert_header(("Connection","keep-alive"))
        .streaming(resp.bytes_stream().map(|r| r.map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))))
}

fn classify_error(c: u16) -> ErrorType { match c { 429 => ErrorType::RateLimit, 408 => ErrorType::Timeout, s if s >= 500 => ErrorType::ServerError, _ => ErrorType::ConnectionError } }
fn should_retry(e: &ErrorType, r: &[ErrorType]) -> bool { r.contains(e) }
