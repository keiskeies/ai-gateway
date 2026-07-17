use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{RouteCache, StatsWriter};
use crate::error::AppError;
use crate::lb::BackendSelector;
use crate::models::route::ErrorType;
use crate::models::stats::RequestStat;
use crate::protocol::{openai, anthropic, types::UnifiedRequest};
use crate::config::AppConfig;

pub struct ProxyState {
    pub db: crate::db::DbPool,
    pub config: AppConfig,
    pub selector: Arc<BackendSelector>,
    pub http_client: reqwest::Client,
    pub cache: RouteCache,
    pub stats_writer: StatsWriter,
}

fn extract_auth(req: &HttpRequest) -> Option<String> {
    req.headers().get("Authorization").and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).to_string())
        .or_else(|| req.headers().get("x-api-key").and_then(|v| v.to_str().ok()).map(String::from))
}

/// Global OpenAI-compatible endpoint: POST /v1/chat/completions
pub async fn openai_chat_completions(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    validate_auth_cached(&state.cache, &auth)?;

    let unified = openai::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::OpenAI).await
}

/// Global Anthropic-compatible endpoint: POST /v1/messages
pub async fn anthropic_messages(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    validate_auth_cached(&state.cache, &auth)?;

    let unified = anthropic::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::Anthropic).await
}

/// List available models (virtual model names)
pub async fn openai_list_models(
    state: web::Data<Arc<ProxyState>>, _req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let db = state.db.clone();
    let proxies = web::block(move || crate::db::proxy::list(&db))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let models: Vec<String> = proxies.iter().map(|p| p.name.clone()).collect();
    Ok(HttpResponse::Ok().json(openai::models_list(&models)))
}

/// 使用缓存验证 API key，不走 DB（极热路径优化）
fn validate_auth_cached(cache: &RouteCache, auth: &Option<String>) -> Result<(), AppError> {
    let valid = match auth.as_deref() {
        Some(key) => cache.validate_api_key(key).map_err(|e| AppError::Internal(e.to_string()))?,
        None => false,
    };
    if valid {
        if let Some(key) = auth {
            cache.touch_api_key(key);
        }
        Ok(())
    } else {
        Err(AppError::BadRequest("Invalid API key".to_string()))
    }
}

async fn handle_request(
    state: &Arc<ProxyState>,
    unified: UnifiedRequest, response_protocol: crate::models::proxy::Protocol,
) -> Result<HttpResponse, AppError> {
    let virtual_model = unified.model.clone();
    let is_stream = unified.stream.unwrap_or(false);

    // 从内存缓存获取路由信息（零 DB 查询）
    let resolved_route = state.cache.get_resolved_route(&virtual_model)
        .ok_or_else(|| AppError::NotFound(format!("Model '{}' not found", virtual_model)))?;

    let max_retries = resolved_route.retry_policy.max_retries;
    let mut last_error: Option<String> = None;

    for attempt in 0..=max_retries {
        let backends_ref: Vec<_> = resolved_route.backends.iter().map(|rb| rb.backend.clone()).collect();
        let backend = state.selector.select(&resolved_route.route_id, &backends_ref, &resolved_route.lb_strategy)
            .ok_or_else(|| AppError::Internal("No available backend".to_string()))?;

        // 从缓存中找到对应的 ResolvedBackend（包含 platform 信息）
        let resolved = resolved_route.backends.iter()
            .find(|rb| rb.backend.id == backend.id)
            .ok_or_else(|| AppError::Internal("Backend not found in cache".to_string()))?;

        state.selector.inc_connection(&resolved_route.route_id, &backend.id);
        let start = Instant::now();

        let (forward_body, forward_url) = build_forward_request(&unified, &resolved.platform, &backend.model_id);

        let mut req_builder = state.http_client.post(&forward_url)
            .timeout(std::time::Duration::from_secs(state.config.defaults.request_timeout_secs))
            .header("Content-Type", "application/json");

        if !resolved.platform.api_key.is_empty() {
            match resolved.platform.platform_type {
                crate::models::platform::PlatformType::Anthropic => {
                    req_builder = req_builder.header("x-api-key", &resolved.platform.api_key).header("anthropic-version", "2023-06-01");
                }
                _ => { req_builder = req_builder.header("Authorization", format!("Bearer {}", resolved.platform.api_key)); }
            }
        }
        if let Some(headers) = resolved.platform.custom_headers.as_object() {
            for (k, v) in headers { if let Some(vs) = v.as_str() { req_builder = req_builder.header(k.as_str(), vs); } }
        }

        let result = req_builder.json(&forward_body).send().await;
        state.selector.dec_connection(&resolved_route.route_id, &backend.id);
        let latency_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if is_stream {
                        // 异步记录统计（不阻塞请求）
                        state.stats_writer.record(RequestStat {
                            id: 0,
                            proxy_id: resolved_route.proxy_id.clone(),
                            route_id: resolved_route.route_id.clone(),
                            backend_id: backend.id.clone(),
                            status_code: 200,
                            latency_ms,
                            token_input: None,
                            token_output: None,
                            error_type: None,
                            created_at: chrono::Utc::now().to_rfc3339(),
                        });
                        return Ok(handle_stream(resp).await);
                    }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let raw: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    let ur = match resolved.platform.platform_type {
                        crate::models::platform::PlatformType::Anthropic => anthropic::parse_response(raw).map_err(AppError::Internal)?,
                        _ => openai::parse_response(raw).map_err(AppError::Internal)?,
                    };

                    let token_input = ur.usage.as_ref().map(|u| u.prompt_tokens as i64);
                    let token_output = ur.usage.as_ref().map(|u| u.completion_tokens as i64);

                    state.stats_writer.record(RequestStat {
                        id: 0,
                        proxy_id: resolved_route.proxy_id.clone(),
                        route_id: resolved_route.route_id.clone(),
                        backend_id: backend.id.clone(),
                        status_code: 200,
                        latency_ms,
                        token_input,
                        token_output,
                        error_type: None,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });

                    let cr = match response_protocol { crate::models::proxy::Protocol::Anthropic => anthropic::to_response(&ur), _ => openai::to_response(&ur) };
                    return Ok(HttpResponse::Ok().json(cr));
                } else {
                    let eb = resp.text().await.unwrap_or_default();
                    let status_code = status.as_u16() as i32;
                    let et = format!("{:?}", classify_error(status.as_u16()));

                    state.stats_writer.record(RequestStat {
                        id: 0,
                        proxy_id: resolved_route.proxy_id.clone(),
                        route_id: resolved_route.route_id.clone(),
                        backend_id: backend.id.clone(),
                        status_code,
                        latency_ms,
                        token_input: None,
                        token_output: None,
                        error_type: Some(et),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });

                    if should_retry(&classify_error(status.as_u16()), &resolved_route.retry_policy.retry_on_error) && attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                state.stats_writer.record(RequestStat {
                    id: 0,
                    proxy_id: resolved_route.proxy_id.clone(),
                    route_id: resolved_route.route_id.clone(),
                    backend_id: backend.id.clone(),
                    status_code: 0,
                    latency_ms,
                    token_input: None,
                    token_output: None,
                    error_type: Some("ConnectionError".to_string()),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });

                last_error = Some(format!("Connection: {}", e));
                if should_retry(&ErrorType::ConnectionError, &resolved_route.retry_policy.retry_on_error) && attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
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
