use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Instant;

use crate::db::DbPool;
use crate::error::AppError;
use crate::lb::BackendSelector;
use crate::models::route::ErrorType;
use crate::models::stats::RequestStat;
use crate::protocol::{openai, anthropic, types::UnifiedRequest};
use crate::config::AppConfig;

pub struct ProxyState {
    pub db: DbPool,
    pub config: AppConfig,
    pub selector: Arc<BackendSelector>,
    pub http_client: reqwest::Client,
}

fn extract_auth(req: &HttpRequest) -> Option<String> {
    req.headers().get("Authorization").and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).to_string())
        .or_else(|| req.headers().get("x-api-key").and_then(|v| v.to_str().ok()).map(String::from))
}

/// Validate auth using api_keys table. If no api_keys exist at all, allow access.
fn validate_auth(db: &DbPool, auth: &Option<String>) -> Result<(), AppError> {
    let keys = crate::db::api_key::list(db).map_err(|e| AppError::Internal(e.to_string()))?;

    // If no API keys configured at all, allow all access
    if keys.is_empty() {
        return Ok(());
    }

    // Otherwise, require a valid key
    let auth_key = auth.as_deref().ok_or_else(|| AppError::BadRequest("API key required".to_string()))?;
    let valid = crate::db::api_key::validate_any(db, auth_key)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if valid {
        // Update last_used
        if let Some(ak) = crate::db::api_key::get_by_key(db, auth_key).ok().flatten() {
            let _ = crate::db::api_key::update_last_used(db, &ak.id);
        }
        Ok(())
    } else {
        Err(AppError::BadRequest("Invalid API key".to_string()))
    }
}

/// Global OpenAI-compatible endpoint: POST /v1/chat/completions
/// Routes to the virtual model (proxy) specified by the "model" field.
pub async fn openai_chat_completions(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    let db = state.db.clone();
    web::block(move || validate_auth(&db, &auth))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let unified = openai::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::OpenAI).await
}

/// Global Anthropic-compatible endpoint: POST /v1/messages
pub async fn anthropic_messages(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    let db = state.db.clone();
    web::block(move || validate_auth(&db, &auth))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let unified = anthropic::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::Anthropic).await
}

/// List available models (virtual model names)
pub async fn openai_list_models(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    let db = state.db.clone();
    web::block(move || validate_auth(&db, &auth))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;

    let db = state.db.clone();
    let proxies = web::block(move || crate::db::proxy::list(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let models: Vec<String> = proxies.iter().map(|p| p.name.clone()).collect();
    Ok(HttpResponse::Ok().json(openai::models_list(&models)))
}

fn record_stat(db: &DbPool, proxy_id: &str, route_id: &str, backend_id: &str,
               status_code: i32, latency_ms: i64, token_input: Option<i64>, token_output: Option<i64>,
               error_type: Option<String>) {
    let stat = RequestStat {
        id: 0,
        proxy_id: proxy_id.to_string(),
        route_id: route_id.to_string(),
        backend_id: backend_id.to_string(),
        status_code,
        latency_ms,
        token_input,
        token_output,
        error_type,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = crate::db::stats::record(db, &stat) {
        tracing::warn!("Failed to record stat: {}", e);
    }
}

async fn handle_request(
    state: &Arc<ProxyState>,
    unified: UnifiedRequest, response_protocol: crate::models::proxy::Protocol,
) -> Result<HttpResponse, AppError> {
    let virtual_model = unified.model.clone();
    let is_stream = unified.stream.unwrap_or(false);

    // Find the proxy (virtual model) by name
    let db = state.db.clone();
    let vm = virtual_model.clone();
    let proxy = web::block(move || crate::db::proxy::get_by_name(&db, &vm))
        .await.map_err(|e| AppError::Internal(e.to_string()))??
        .ok_or_else(|| AppError::NotFound(format!("Model '{}' not found", virtual_model)))?;

    // Find the route for this proxy
    let db = state.db.clone();
    let proxy_id = proxy.id.clone();
    let route = web::block(move || crate::db::route::get_by_proxy(&db, &proxy_id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??
        .ok_or_else(|| AppError::NotFound(format!("No route configured for model '{}'", virtual_model)))?;

    let max_retries = route.retry_policy.max_retries;
    let mut last_error: Option<String> = None;

    for attempt in 0..=max_retries {
        let backend = state.selector.select(&route.id, &route.backends, &route.lb_strategy)
            .ok_or_else(|| AppError::Internal("No available backend".to_string()))?;

        let db = state.db.clone();
        let platform_id = backend.platform_id.clone();
        let platform = web::block(move || crate::db::platform::get(&db, &platform_id))
            .await.map_err(|e| AppError::Internal(e.to_string()))??;

        // backend.model_id now stores the actual model ID string directly (e.g. "gpt-4o")
        let model_id_str = backend.model_id.clone();

        state.selector.inc_connection(&route.id, &backend.id);
        let start = Instant::now();

        let (forward_body, forward_url) = build_forward_request(&unified, &platform, &model_id_str);

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
        let latency_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if is_stream {
                        let db = state.db.clone();
                        let pid = proxy.id.clone();
                        let rid = route.id.clone();
                        let bid = backend.id.clone();
                        web::block(move || record_stat(&db, &pid, &rid, &bid, 200, latency_ms, None, None, None)).await.ok();
                        return Ok(handle_stream(resp).await);
                    }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let raw: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    let ur = match platform.platform_type {
                        crate::models::platform::PlatformType::Anthropic => anthropic::parse_response(raw).map_err(AppError::Internal)?,
                        _ => openai::parse_response(raw).map_err(AppError::Internal)?,
                    };

                    let token_input = ur.usage.as_ref().map(|u| u.prompt_tokens as i64);
                    let token_output = ur.usage.as_ref().map(|u| u.completion_tokens as i64);

                    let db = state.db.clone();
                    let pid = proxy.id.clone();
                    let rid = route.id.clone();
                    let bid = backend.id.clone();
                    web::block(move || record_stat(&db, &pid, &rid, &bid, 200, latency_ms, token_input, token_output, None)).await.ok();

                    let cr = match response_protocol { crate::models::proxy::Protocol::Anthropic => anthropic::to_response(&ur), _ => openai::to_response(&ur) };
                    return Ok(HttpResponse::Ok().json(cr));
                } else {
                    let eb = resp.text().await.unwrap_or_default();
                    let status_code = status.as_u16() as i32;

                    let db = state.db.clone();
                    let pid = proxy.id.clone();
                    let rid = route.id.clone();
                    let bid = backend.id.clone();
                    let et = format!("{:?}", classify_error(status.as_u16()));
                    web::block(move || record_stat(&db, &pid, &rid, &bid, status_code, latency_ms, None, None, Some(et))).await.ok();

                    if should_retry(&classify_error(status.as_u16()), &route.retry_policy.retry_on_error) && attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                let db = state.db.clone();
                let pid = proxy.id.clone();
                let rid = route.id.clone();
                let bid = backend.id.clone();
                web::block(move || record_stat(&db, &pid, &rid, &bid, 0, latency_ms, None, None, Some("ConnectionError".to_string()))).await.ok();

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
