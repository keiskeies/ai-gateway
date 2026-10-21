use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use crate::cache::{RouteCache, StatsWriter};
use crate::error::AppError;
use crate::lb::BackendSelector;
use crate::models::route::ErrorType;
use crate::models::stats::{RequestAttempt, RequestStat};
use crate::protocol::{openai, anthropic, types::UnifiedRequest};

pub struct ProxyState {
    pub db: crate::db::DbPool,
    /// 与 `/api/settings` 共用同一份 `Arc<RwLock<_>>`，不是启动时的快照：
    /// 正文采集这类开关必须改完就生效——"关掉之后还在偷偷存正文"是最不能接受的方向。
    pub config: crate::api::settings::SharedAppConfig,
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
    let auth_key = validate_auth_cached(&state.cache, &auth)?;

    // 解析会吃掉这份 JSON，所以要在交给解析器之前留一份原样正文（采集开着才用得上）
    let raw = body.0.clone();
    let unified = openai::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::OpenAI, auth_key, Some(raw), None).await
}

/// Global Anthropic-compatible endpoint: POST /v1/messages
pub async fn anthropic_messages(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    let auth_key = validate_auth_cached(&state.cache, &auth)?;

    let raw = body.0.clone();
    let unified = anthropic::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::Anthropic, auth_key, Some(raw), None).await
}

/// 正文采集上限：再长的上下文也不该让正文表无限膨胀，而且超长请求也不是
/// "点一下重放看看"的那种。超限就不存，重放按钮对那一行保持不可用。
pub const MAX_CAPTURED_BODY_BYTES: usize = 64 * 1024;

/// 采集开着时把入站正文投进异步通道；关着时什么都不做。
/// 默认不存正文是硬口径（§2 原则 10），所以这里只有"显式开启才写"一条路径，
/// 且与统计一样绝不阻塞请求线程。
fn capture_body(
    state: &Arc<ProxyState>,
    request_id: &str,
    protocol: &str,
    raw: &Option<serde_json::Value>,
) {
    let Some(value) = raw else { return };
    // 读锁只在这条 if 里持有，取到 bool 就释放——后面全是 await，不能跨着拿锁
    let capture = state.config.read().defaults.capture_request_bodies;
    if !capture {
        return;
    }
    let text = value.to_string();
    let bytes = text.len() as i64;
    if bytes > MAX_CAPTURED_BODY_BYTES as i64 {
        tracing::debug!("请求正文 {} 字节超过上限，未存正文（request_id={}）", bytes, request_id);
        return;
    }
    state.stats_writer.record_body(crate::models::stats::RequestBody {
        request_id: request_id.to_string(),
        protocol: protocol.to_string(),
        body: text,
        bytes,
        created_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// List available models (virtual model names) with capability metadata
pub async fn openai_list_models(
    state: web::Data<Arc<ProxyState>>, _req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let db = state.db.clone();
    let models = web::block(move || -> Result<Vec<openai::ModelInfo>, AppError> {
        let proxies = crate::db::proxy::list(&db)?;
        let mut out = Vec::new();
        for proxy in proxies {
            let mut intersection: Option<std::collections::HashSet<String>> = None;
            if let Ok(Some(route)) = crate::db::route::get_by_proxy(&db, &proxy.id) {
                let backends = crate::db::route::list_backends_by_route(&db, &route.id).unwrap_or_default();
                for backend in &backends {
                    let caps: std::collections::HashSet<String> =
                        backend.capabilities.iter().cloned().collect();
                    intersection = Some(match intersection {
                        None => caps,
                        Some(prev) => prev.intersection(&caps).cloned().collect(),
                    });
                }
            }
            out.push(openai::ModelInfo {
                name: proxy.name,
                capabilities: intersection.map(|s| s.into_iter().collect()).unwrap_or_default(),
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(openai::models_list(&models)))
}

/// 使用缓存验证 API key，不走 DB（极热路径优化）。
/// 返回匹配的 key 供后续校验 proxy 绑定；未配置任何 key 时返回 None（兼容旧行为）。
/// 客户端的明文在这里只参与一次摘要计算，之后不再被持有。
fn validate_auth_cached(
    cache: &RouteCache,
    auth: &Option<String>,
) -> Result<Option<crate::models::api_key::StoredApiKey>, AppError> {
    let Some(key) = auth.as_deref() else {
        return Err(AppError::BadRequest("Missing API key".to_string()));
    };
    if !cache.has_api_keys() {
        return Ok(None);
    }
    let api_key = match cache.find_enabled_api_key(key) {
        Some(api_key) => api_key,
        // 到点是硬失效，不是"缓存还没刷"：把过期与"无效/已禁用"分开报，
        // 否则调用方只会照着禁用去排查，永远猜不到是日期过了。
        None if cache.find_api_key(key).map(|k| k.is_expired(chrono::Utc::now())).unwrap_or(false) => {
            return Err(AppError::Unauthorized("API key expired".to_string()));
        }
        None => return Err(AppError::BadRequest("Invalid or disabled API key".to_string())),
    };
    cache.touch_api_key(&api_key.id);
    Ok(Some(api_key))
}

/// 配额判定：只有设了配额的 Key 才会多走这一次查询，其余 Key 的开销与改动前完全一致。
/// 用量口径与 `/keys` 列表里显示的数字同源（同一份 `request_stats`、同一个时间窗），
/// 所以界面显示 42/100 与第 101 次被拒不会互相矛盾。
async fn enforce_quota(
    state: &Arc<ProxyState>,
    api_key: &Option<crate::models::api_key::StoredApiKey>,
) -> Result<(), AppError> {
    let Some(key) = api_key.as_ref() else { return Ok(()) };
    if !key.has_quota() {
        return Ok(());
    }
    let usage = usage_for(state, &key.id, key.quota_window.as_deref()).await?;
    if key.quota_requests.map(|max| usage.requests >= max).unwrap_or(false) {
        return Err(AppError::TooManyRequests("API key request quota exceeded".to_string()));
    }
    if key.quota_tokens.map(|max| usage.tokens >= max).unwrap_or(false) {
        return Err(AppError::TooManyRequests("API key token quota exceeded".to_string()));
    }
    // 金额配额只认已定价的请求：没录价格的模型算不出成本，也就不会被这条闸门拒。
    // 这是取舍，不是漏判——宁可少拒，也不要用一个猜出来的单价去断掉真实请求。
    if let (Some(max), Some(spent)) = (key.quota_cost, usage.cost) {
        if spent >= max {
            return Err(AppError::TooManyRequests("API key cost quota exceeded".to_string()));
        }
    }
    Ok(())
}

async fn usage_for(state: &Arc<ProxyState>, id: &str, window: Option<&str>) -> Result<crate::models::api_key::KeyUsage, AppError> {
    let db = state.db.clone();
    let id = id.to_string();
    let window = window.map(str::to_string);
    web::block(move || crate::db::api_key::usage_for_key(&db, &id, window.as_deref()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// 重放一条存过正文的请求：把当时的原始正文重新送进网关，路由、负载均衡、重试、
/// 统计与轨迹全部按今天的配置再走一遍——不是回放上游那一份响应，上游每次都会重新生成。
///
/// 调用方是会话鉴权后的管理接口，所以 `auth_key` 传 None：V17 之后库里只有 Key 的哈希，
/// 本来也还原不出当初那条明文；配额与模型绑定检查对 None 一律放行（见 enforce_quota）。
/// 新那条统计行带 `replayed_from`，界面上能看出它是重放出来的。
pub async fn replay_request(
    state: &Arc<ProxyState>,
    stat_id: i64,
    stored: &crate::models::stats::RequestBody,
) -> Result<HttpResponse, AppError> {
    let value: serde_json::Value = serde_json::from_str(&stored.body)
        .map_err(|e| AppError::Internal(format!("stored request body is not valid JSON: {}", e)))?;
    let (unified, protocol) = match stored.protocol.as_str() {
        "anthropic" => (
            anthropic::parse_request(value.clone()).map_err(AppError::BadRequest)?,
            crate::models::proxy::Protocol::Anthropic,
        ),
        _ => (
            openai::parse_request(value.clone()).map_err(AppError::BadRequest)?,
            crate::models::proxy::Protocol::OpenAI,
        ),
    };
    handle_request(state, unified, protocol, None, Some(value), Some(stat_id)).await
}

/// 构造一条统计记录
fn make_stat(
    proxy_id: &str,
    route_id: &str,
    backend_id: &str,
    status_code: i32,
    latency_ms: i64,
    token_input: Option<i64>,
    token_output: Option<i64>,
    error_type: Option<String>,
    is_stream: bool,
    api_key_id: Option<&str>,
    request_id: &str,
    replayed_from: Option<i64>,
) -> RequestStat {
    RequestStat {
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
        ttft_ms: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        is_stream,
        api_key_id: api_key_id.map(|s| s.to_string()),
        // 成本由调用方在拿到 token 后填写；这里还不知道用量
        cost: None,
        request_id: Some(request_id.to_string()),
        replayed_from,
    }
}

/// 构造一条尝试轨迹：只带元数据（后端/平台/上游模型名/状态/错误摘要/耗时），
/// 请求与响应正文一律不落库；错误文本由调用方先做截断与密钥脱敏
fn make_attempt(
    request_id: &str,
    attempt_no: i64,
    backend: &crate::models::route::Backend,
    status_code: Option<i64>,
    error_type: Option<&str>,
    error_message: Option<String>,
    latency_ms: i64,
    started_at: String,
) -> RequestAttempt {
    RequestAttempt {
        id: 0,
        request_id: request_id.to_string(),
        attempt_no,
        backend_id: Some(backend.id.clone()),
        platform_id: Some(backend.platform_id.clone()),
        model_id: Some(backend.model_id.clone()),
        status_code,
        error_type: error_type.map(|s| s.to_string()),
        error_message,
        latency_ms: Some(latency_ms),
        started_at,
        platform_name: None,
    }
}

/// 错误摘要：截到 300 字符，并把平台密钥替换成占位符（个别上游 401 会回显收到的 key）
fn clip_error(text: &str, secret: &str) -> String {
    let head: String = text.chars().take(300).collect();
    if secret.is_empty() {
        head
    } else {
        head.replace(secret, "[redacted]")
    }
}

async fn handle_request(
    state: &Arc<ProxyState>,
    unified: UnifiedRequest,
    response_protocol: crate::models::proxy::Protocol,
    auth_key: Option<crate::models::api_key::StoredApiKey>,
    raw_body: Option<serde_json::Value>,
    replayed_from: Option<i64>,
) -> Result<HttpResponse, AppError> {
    let virtual_model = unified.model.clone();
    let is_stream = unified.stream.unwrap_or(false);
    // 统计归属到发起请求的密钥（未鉴权时为 None）
    let api_key_id = auth_key.as_ref().map(|k| k.id.clone());
    enforce_quota(state, &auth_key).await?;

    // 从内存缓存获取路由信息（零 DB 查询）
    let resolved_route = state.cache.get_resolved_route(&virtual_model)
        .ok_or_else(|| AppError::NotFound(format!("Model '{}' not found", virtual_model)))?;

    // Key 绑定某个虚拟模型时，禁止访问其他模型。
    if let Some(api_key) = auth_key.as_ref() {
        if let Some(bound_proxy) = api_key.proxy_id.as_deref() {
            if bound_proxy != resolved_route.proxy_id {
                return Err(AppError::Unauthorized(
                    "API key is not allowed for this model".to_string(),
                ));
            }
        }
    }

    let max_retries = resolved_route.retry_policy.max_retries;
    let mut last_error: Option<String> = None;
    // 一次入站请求一个 ID：所有重试尝试共享，轨迹接口按它串起依次试过的后端
    let request_id = uuid::Uuid::new_v4().to_string();
    // 正文按这个 request_id 挂到那条统计行上；放在路由解析之后，是为了不给
    // "模型不存在 / Key 不允许"这类根本没进转发的请求存正文。
    // 重放那一笔也不存：它的正文与当初那份逐字相同，再存一遍只是把同一句话复制一份，
    // 要看第二次就重放当初那条（重放行上的"重放自 #N"会指回去）。
    if replayed_from.is_none() {
        capture_body(
            state,
            &request_id,
            match response_protocol {
                crate::models::proxy::Protocol::OpenAI => "openai",
                crate::models::proxy::Protocol::Anthropic => "anthropic",
            },
            &raw_body,
        );
    }

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
        let started_at = chrono::Utc::now().to_rfc3339();
        let attempt_no = attempt as i64 + 1;

        let (forward_body, forward_url) = build_forward_request(&unified, &resolved.platform, &resolved.backend, resolved_route.default_max_tokens);

        let mut req_builder = state.http_client.post(&forward_url)
            // 流式长生成（推理模型/长输出）远超 request_timeout，单独放宽到 stream_timeout
            .timeout(std::time::Duration::from_secs(if is_stream {
                state.config.read().defaults.stream_timeout_secs
            } else {
                state.config.read().defaults.request_timeout_secs
            }))
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
                    // 拿到响应头即算本次尝试成功（后续解析失败不改上游侧结论，也不额外补记）
                    state.stats_writer.record_attempt(make_attempt(
                        &request_id,
                        attempt_no,
                        &backend,
                        Some(status.as_u16() as i64),
                        None,
                        None,
                        latency_ms,
                        started_at.clone(),
                    ));
                    if is_stream {
                        // 流式：统计由 handle_stream / handle_stream_convert 在流结束时记录一次（含 SSE usage token）
                        let base = make_stat(
                            &resolved_route.proxy_id,
                            &resolved_route.route_id,
                            &backend.id,
                            200,
                            latency_ms,
                            None,
                            None,
                            None,
                            true,
                            api_key_id.as_deref(),
                            &request_id,
                            replayed_from,
                        );
                        // 协议匹配 → 透传；跨协议 → 实时转换（Anthropic 事件流 ↔ OpenAI chunk 流）
                        let client_anthropic = matches!(response_protocol, crate::models::proxy::Protocol::Anthropic);
                        let upstream_anthropic = matches!(resolved.platform.platform_type, crate::models::platform::PlatformType::Anthropic);
                        if client_anthropic == upstream_anthropic {
                            return Ok(handle_stream(resp, &state.stats_writer, base, backend.price_pair(), start));
                        }
                        tracing::info!("跨协议流式转换：客户端={} 上游={}",
                            if client_anthropic { "anthropic" } else { "openai" },
                            if upstream_anthropic { "anthropic" } else { "openai" });
                        return Ok(handle_stream_convert(resp, &state.stats_writer, base, backend.price_pair(), client_anthropic, &virtual_model, &backend.id, &backend.model_id, start));
                    }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let raw: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    let upstream_anthropic = matches!(resolved.platform.platform_type, crate::models::platform::PlatformType::Anthropic);
                    let same_protocol = matches!(response_protocol, crate::models::proxy::Protocol::Anthropic) == upstream_anthropic;
                    // 解析：按上游平台协议解析为统一格式（同时用于统计与签名缓存）
                    let parsed = match resolved.platform.platform_type {
                        crate::models::platform::PlatformType::Anthropic => match anthropic::parse_response(raw.clone()) {
                            Ok(ur) => ur,
                            Err(e) => {
                                tracing::warn!("解析 Anthropic 响应失败（透传原始响应）: {}", e);
                                if same_protocol {
                                    return Ok(passthrough_response(raw, &virtual_model));
                                }
                                return Ok(HttpResponse::Ok().json(raw));
                            }
                        },
                        _ => match openai::parse_response(raw.clone()) {
                            Ok(ur) => ur,
                            Err(e) => {
                                tracing::warn!("解析 OpenAI 响应失败（透传原始响应）: {}", e);
                                if same_protocol {
                                    return Ok(passthrough_response(raw, &virtual_model));
                                }
                                return Ok(HttpResponse::Ok().json(raw));
                            }
                        },
                    };
                    let token_input = parsed.usage.as_ref().map(|u| u.prompt_tokens as i64);
                    let token_output = parsed.usage.as_ref().map(|u| u.completion_tokens as i64);
                    let cache_read = parsed.usage.as_ref().and_then(|u| u.cache_read_tokens).map(|v| v as i64);
                    let cache_write = parsed.usage.as_ref().and_then(|u| u.cache_write_tokens).map(|v| v as i64);

                    // Anthropic 上游 + tool_use：缓存 thinking 签名，供下一轮 OpenAI 客户端工具调用回放
                    if upstream_anthropic {
                        if let Some(msg) = parsed.choices.first().and_then(|c| c.message.as_ref()) {
                            crate::proxy::reasoning_cache::cache_assistant_message(msg, &backend.id, &backend.model_id);
                        }
                    }

                    let mut stat = make_stat(
                        &resolved_route.proxy_id,
                        &resolved_route.route_id,
                        &backend.id,
                        200,
                        latency_ms,
                        token_input,
                        token_output,
                        None,
                        false,
                        api_key_id.as_deref(),
                        &request_id,
                        replayed_from,
                    );
                    stat.cache_read_tokens = cache_read;
                    stat.cache_write_tokens = cache_write;
                    stat.cost = backend.estimate_cost(token_input, token_output);
                    state.stats_writer.record(stat);

                    // 同协议非流式：原始响应体透传（仅把 model 改回虚拟模型名），避免重建丢失 thinking/未知字段
                    if same_protocol {
                        return Ok(passthrough_response(raw, &virtual_model));
                    }
                    let cr = match response_protocol {
                        crate::models::proxy::Protocol::Anthropic => anthropic::to_response(&parsed),
                        _ => openai::to_response(&parsed),
                    };
                    return Ok(HttpResponse::Ok().json(cr));
                } else {
                    // 上游业务失败：先落本次尝试轨迹，再判断是否重试（统计仍只在最终结果记录，避免重试重复计数）
                    let et = format!("{:?}", classify_error(status.as_u16()));
                    last_error = Some(format!("upstream {} ({})", status.as_u16(), et));
                    let eb = resp.text().await.unwrap_or_default();
                    state.stats_writer.record_attempt(make_attempt(
                        &request_id,
                        attempt_no,
                        &backend,
                        Some(status.as_u16() as i64),
                        Some(&et),
                        // 上游偶尔回空正文：没有内容就别存空串，前端按"只有错误类型"渲染
                        (!eb.is_empty()).then(|| clip_error(&eb, &resolved.platform.api_key)),
                        latency_ms,
                        started_at.clone(),
                    ));
                    if should_retry(&classify_error(status.as_u16()), &resolved_route.retry_policy.retry_on_error) && attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    // 最终失败：记录一条统计并透传原始响应体
                    state.stats_writer.record(make_stat(
                        &resolved_route.proxy_id,
                        &resolved_route.route_id,
                        &backend.id,
                        status.as_u16() as i32,
                        latency_ms,
                        None,
                        None,
                        Some(et),
                        is_stream,
                        api_key_id.as_deref(),
                        &request_id,
                        replayed_from,
                    ));
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                // 连接错误：轨迹逐次记录（含被重试的那几次），统计只在最终结果落一条，避免重试重复计数
                last_error = Some(format!("Connection: {}", e));
                state.stats_writer.record_attempt(make_attempt(
                    &request_id,
                    attempt_no,
                    &backend,
                    None,
                    Some("ConnectionError"),
                    Some(clip_error(&e.to_string(), &resolved.platform.api_key)),
                    latency_ms,
                    started_at.clone(),
                ));
                if should_retry(&ErrorType::ConnectionError, &resolved_route.retry_policy.retry_on_error) && attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                    continue;
                }
                // 最终失败：记录一条连接错误统计（后端已在本次 attempt 中选定）
                state.stats_writer.record(make_stat(
                    &resolved_route.proxy_id,
                    &resolved_route.route_id,
                    &backend.id,
                    0,
                    latency_ms,
                    None,
                    None,
                    Some("ConnectionError".to_string()),
                    is_stream,
                    api_key_id.as_deref(),
                    &request_id,
                    replayed_from,
                ));
            }
        }
    }
    Err(AppError::Internal(format!("All retries: {}", last_error.unwrap_or_default())))
}

fn build_forward_request(
    unified: &UnifiedRequest,
    platform: &crate::models::platform::Platform,
    backend: &crate::models::route::Backend,
    default_max_tokens: Option<u32>,
) -> (serde_json::Value, String) {
    use crate::protocol::reasoning::{resolve_style, ReasoningTarget};
    // 思考参数风格：Auto 按目标平台解析；客户端未传思考配置时不会主动开启
    let style = resolve_style(&backend.reasoning_style, &platform.platform_type);
    let target = ReasoningTarget::new(
        style,
        backend.id.clone(),
        backend.model_id.clone(),
        backend.reasoning_echo.clone(),
    );
    let (mut body, url) = match platform.platform_type {
        crate::models::platform::PlatformType::Anthropic => {
            (anthropic::to_request(unified, &backend.model_id, default_max_tokens, backend.unknown_params.clone(), &target), format!("{}/v1/messages", platform.base_url.trim_end_matches('/')))
        }
        _ => {
            (openai::to_request(unified, &backend.model_id, backend.unknown_params.clone(), &target), format!("{}/chat/completions", platform.base_url.trim_end_matches('/')))
        }
    };
    // 流式请求注入 stream_options.include_usage：OpenAI 兼容上游默认不在流内返回 usage chunk，
    // 不注入则网关旁路解析不到 token，统计里输入/输出全为 0（速度也因分母缺失显示 —）。
    // 客户端已带 stream_options 时不覆盖；Anthropic 流式始终在 message_start/delta 带 usage，无需注入。
    // 个别不兼容该字段的上游，可用 backend 参数覆盖强制改写（param_overrides 在其后 merge，优先级更高）。
    if platform.platform_type != crate::models::platform::PlatformType::Anthropic {
        if let Some(body_obj) = body.as_object_mut() {
            if body_obj.get("stream").and_then(|v| v.as_bool()) == Some(true)
                && !body_obj.contains_key("stream_options")
            {
                body_obj.insert("stream_options".into(), serde_json::json!({ "include_usage": true }));
            }
        }
    }
    // backend 级参数覆盖：最后 merge，覆盖优先（可强制 temperature / 注入平台特有字段等）
    if let Some(overrides) = backend.param_overrides.as_object() {
        if let Some(body_obj) = body.as_object_mut() {
            for (k, v) in overrides {
                body_obj.insert(k.clone(), v.clone());
            }
        }
    }
    // 思考映射观测：只记录参数形态，不记录思考内容
    if let Some(cfg) = &unified.reasoning {
        tracing::debug!(
            "思考映射：source={:?} style={:?} enabled={} effort={:?} budget={:?} out_thinking={} out_effort={} out_reasoning={}",
            cfg.source,
            target.style,
            cfg.is_enabled(),
            cfg.effective_effort(),
            cfg.effective_budget_tokens(),
            body.get("thinking").map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            body.get("reasoning_effort").map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            body.get("reasoning").map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        );
    }
    // 部分上游（agnes）无法处理单通道灰度图，会返回 `mean must have 1 elements ...`，转发前转成 RGB
    let converted = crate::protocol::content::normalize_images_in_body(&mut body);
    if converted > 0 {
        tracing::info!("图片规范化：{} 张单通道图片已转换为 RGB（上游不支持灰度图）", converted);
    }
    (body, url)
}

/// 流式响应透传 body：透传 SSE 给客户端，同时旁路解析流内 usage 累积 token。
/// 统计恰好记录一次：流正常结束、上游出错、或客户端断开（body drop）时都触发。
struct StreamPassthroughBody {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>,
    buffer: String,
    stats: Arc<StreamStatCtx>,
}

/// body 被 drop 时兜底记录统计（覆盖客户端中途断开的情况），record_once 用 CAS 保证恰好一次
impl Drop for StreamPassthroughBody {
    fn drop(&mut self) {
        self.stats.record_once();
    }
}

/// 流式统计的共享上下文：token 累积 + 首字时长 + 恰好一次写入
struct StreamStatCtx {
    base: RequestStat,
    /// 发出请求的时刻（用于计算首字时长）
    start: Instant,
    /// 首字时长：收到上游第一个 chunk 时写入（0 表示尚未到达）
    first_token_ms: std::sync::atomic::AtomicI64,
    token_in: std::sync::atomic::AtomicI64,
    token_out: std::sync::atomic::AtomicI64,
    cache_read: std::sync::atomic::AtomicI64,
    cache_write: std::sync::atomic::AtomicI64,
    failed: std::sync::atomic::AtomicBool,
    recorded: std::sync::atomic::AtomicBool,
    /// 单价对（每 100 万 token）：token 在流结束时才知道，所以价格先随上下文带进来
    prices: Option<(f64, f64)>,
    writer: StatsWriter,
}

impl StreamStatCtx {
    /// start 必须是请求发出时刻（与 latency_ms 同源），首字时长以此为基准
    fn new(base: RequestStat, writer: &StatsWriter, start: Instant, prices: Option<(f64, f64)>) -> Self {
        Self {
            base,
            start,
            first_token_ms: std::sync::atomic::AtomicI64::new(0),
            token_in: std::sync::atomic::AtomicI64::new(0),
            token_out: std::sync::atomic::AtomicI64::new(0),
            cache_read: std::sync::atomic::AtomicI64::new(0),
            cache_write: std::sync::atomic::AtomicI64::new(0),
            failed: std::sync::atomic::AtomicBool::new(false),
            recorded: std::sync::atomic::AtomicBool::new(false),
            prices,
            writer: writer.clone(),
        }
    }

    /// 记录首字时长：仅第一个 chunk 生效（CAS 0→elapsed，后续到达为 no-op）
    fn mark_first_token(&self) {
        use std::sync::atomic::Ordering;
        if self.first_token_ms.load(Ordering::SeqCst) > 0 {
            return;
        }
        let elapsed = self.start.elapsed().as_millis() as i64;
        // 期望当前为 0（未记录过）才写入
        let _ = self.first_token_ms.compare_exchange(
            0,
            elapsed.max(1),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    /// 吸收一帧 usage：(input, output, cache_read, cache_write)。
    /// usage 帧携带的是累计总量而非增量，fetch_max 保证多帧/重复帧幂等。
    fn absorb_usage(&self, pi: i64, po: i64, cr: i64, cw: i64) {
        use std::sync::atomic::Ordering;
        self.token_in.fetch_max(pi, Ordering::SeqCst);
        self.token_out.fetch_max(po, Ordering::SeqCst);
        self.cache_read.fetch_max(cr, Ordering::SeqCst);
        self.cache_write.fetch_max(cw, Ordering::SeqCst);
    }

    /// 记录最终统计（恰好一次，用 CAS 保证幂等）
    fn record_once(&self) {
        use std::sync::atomic::Ordering;
        if self.recorded.swap(true, Ordering::SeqCst) {
            return; // 已记录过，忽略
        }
        let mut stat = self.base.clone();
        stat.token_input = Some(self.token_in.load(Ordering::SeqCst));
        stat.token_output = Some(self.token_out.load(Ordering::SeqCst));
        stat.cost = crate::models::route::cost_from_prices(self.prices, stat.token_input, stat.token_output);
        // 缓存命中仅在流内出现过 usage 时非 0
        let cr = self.cache_read.load(Ordering::SeqCst);
        let cw = self.cache_write.load(Ordering::SeqCst);
        stat.cache_read_tokens = if cr > 0 { Some(cr) } else { None };
        stat.cache_write_tokens = if cw > 0 { Some(cw) } else { None };
        // 流式延迟统一口径：base 里的 latency_ms 是"响应头到达"时刻（TTFB），
        // 此处改写为"流结束"完整耗时，使非流式与流式的 latency_ms 含义一致
        stat.latency_ms = self.start.elapsed().as_millis() as i64;
        let ttft = self.first_token_ms.load(Ordering::SeqCst);
        stat.ttft_ms = if ttft > 0 { Some(ttft) } else { None };
        if self.failed.load(Ordering::SeqCst) {
            stat.status_code = 0;
            stat.error_type = Some("StreamError".to_string());
        }
        self.writer.record(stat);
    }
}

impl Stream for StreamPassthroughBody {
    type Item = Result<Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::sync::atomic::Ordering;
        match self.inner.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                // 流正常结束：把 buffer 中残留的半行也解析一次（最后一个 chunk 可能无换行）
                if !self.buffer.is_empty() {
                    let payload = self.buffer.trim().strip_prefix("data:").map(|s| s.trim());
                    if let Some(p) = payload {
                        if let Some((pi, po, cr, cw)) = extract_stream_usage(p) {
                            self.stats.absorb_usage(pi, po, cr, cw);
                        }
                    }
                }
                self.stats.record_once();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(bytes))) => {
                // 收到上游第一个数据 chunk：记录首字时长（CAS 幂等）
                self.stats.mark_first_token();
                self.buffer.push_str(&String::from_utf8_lossy(&bytes));
                // 逐行解析已完整的 SSE 行，避免把半截事件当作独立事件
                while let Some(nl) = self.buffer.find('\n') {
                    let line: String = self.buffer.drain(..=nl).collect();
                    let payload = match line.trim().strip_prefix("data:") {
                        Some(rest) => rest.trim(),
                        None => continue,
                    };
                    if let Some((p, c, cr, cw)) = extract_stream_usage(payload) {
                        self.stats.absorb_usage(p, c, cr, cw);
                    }
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(_))) => {
                // 上游流错误：标记失败、记录统计，并透传错误
                self.stats.failed.store(true, Ordering::SeqCst);
                self.stats.record_once();
                Poll::Ready(Some(Err(actix_web::error::ErrorInternalServerError(
                    "upstream stream error",
                ))))
            }
        }
    }
}

/// 处理流式响应：透传 SSE 给客户端，同时旁路解析流内 usage 累积 token。
/// 统计在流结束时恰好记录一次（不随重试路径重复计数）。
fn handle_stream(
    resp: reqwest::Response,
    stats_writer: &StatsWriter,
    stat_base: RequestStat,
    prices: Option<(f64, f64)>,
    start: Instant,
) -> HttpResponse {
    let s = resp.status();
    if !s.is_success() {
        // 早期失败：记一条失败统计并透传上游状态码
        let mut st = stat_base;
        st.status_code = s.as_u16() as i32;
        st.error_type = Some("UpstreamError".to_string());
        stats_writer.record(st);
        return HttpResponse::build(
            actix_web::http::StatusCode::from_u16(s.as_u16())
                .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
        )
        .body("upstream stream error");
    }

    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start, prices));
    let body = StreamPassthroughBody {
        inner: Box::pin(resp.bytes_stream()),
        buffer: String::new(),
        stats,
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(body)
}

/// 跨协议流式转换 body：上游 SSE → StreamConverter 实时转换为客户端协议事件流，
/// 统计在流结束时恰好记录一次（含转换器提取的 usage token）。
struct StreamConvertBody {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>,
    converter: crate::proxy::stream_convert::StreamConverter,
    stats: Arc<StreamStatCtx>,
}

impl Drop for StreamConvertBody {
    fn drop(&mut self) {
        // 客户端中途断开时兜底记录（record_once CAS 保证幂等）
        self.stats.record_once();
    }
}

impl Stream for StreamConvertBody {
    type Item = Result<Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::sync::atomic::Ordering;
        loop {
            match self.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    // 上游流结束：补发收尾事件，记统计，正常关流
                    let out = self.converter.finish();
                    let (pi, po, cr, cw) = self.converter.usage();
                    self.stats.absorb_usage(pi, po, cr, cw);
                    if !out.is_empty() {
                        // 先记统计再返回收尾帧：下游可能不再 poll（避免依赖 poll-after-None 行为）
                        self.stats.record_once();
                        return Poll::Ready(Some(Ok(Bytes::from(out))));
                    }
                    self.stats.record_once();
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    // 收到上游第一个数据 chunk：记录首字时长（CAS 幂等）
                    self.stats.mark_first_token();
                    let out = self.converter.push_chunk(&bytes);
                    if out.is_empty() { continue; } // 事件未完整，继续读上游
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Err(e))) => {
                    // 上游流错误：按客户端协议发错误帧并正常收尾（SSE 已开始无法改状态码）
                    tracing::warn!("跨协议流式上游错误: {}", e);
                    self.stats.failed.store(true, Ordering::SeqCst);
                    let out = self.converter.emit_error("upstream stream error");
                    let (pi, po, cr, cw) = self.converter.usage();
                    self.stats.absorb_usage(pi, po, cr, cw);
                    self.stats.record_once();
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
            }
        }
    }
}

/// 处理跨协议流式响应：实时把上游 SSE 转换为客户端协议的事件流
fn handle_stream_convert(
    resp: reqwest::Response,
    stats_writer: &StatsWriter,
    stat_base: RequestStat,
    prices: Option<(f64, f64)>,
    client_anthropic: bool,
    model: &str,
    backend_id: &str,
    upstream_model: &str,
    start: Instant,
) -> HttpResponse {
    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start, prices));
    let body = StreamConvertBody {
        inner: Box::pin(resp.bytes_stream()),
        converter: crate::proxy::stream_convert::StreamConverter::new(
            client_anthropic,
            model,
            backend_id,
            upstream_model,
        ),
        stats,
    };
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(body)
}

/// 从流式 SSE 数据中提取 usage（流结束后的统计用）。
/// 返回 (input, output, cache_read, cache_write)。
/// 支持 OpenAI 风格（chunk 里的 usage.prompt_tokens/completion_tokens）
/// 和 Anthropic 风格（message_start 的 input_tokens / message_delta 的 output_tokens）。
fn extract_stream_usage(payload: &str) -> Option<(i64, i64, i64, i64)> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;

    // OpenAI 风格：chunk 顶层带 usage
    if let Some(u) = v.get("usage").and_then(|u| u.as_object()) {
        let p = u.get("prompt_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
        let c = u.get("completion_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
        // 缓存命中：OpenAI 标准 prompt_tokens_details.cached_tokens，兜底 DeepSeek 老格式
        let cr = u.get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens")).and_then(|x| x.as_i64())
            .or_else(|| u.get("prompt_cache_hit_tokens").and_then(|x| x.as_i64()))
            .unwrap_or(0);
        if p > 0 || c > 0 || cr > 0 { return Some((p, c, cr, 0)); }
    }

    // Anthropic 风格：SSE event 为 "type: message_start / message_delta"，data 内携带 usage
    let event_type = v.get("type").and_then(|x| x.as_str());
    match event_type {
        Some("message_start") => {
            let u = v.get("message")?.get("usage")?.as_object()?;
            let input = u.get("input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
            let cr = u.get("cache_read_input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
            let cw = u.get("cache_creation_input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
            // Anthropic 的 input_tokens 不含缓存部分，统一为"总提示词"口径
            Some((input + cr + cw, 0, cr, cw))
        }
        Some("message_delta") => {
            let u = v.get("usage")?.as_object()?;
            let c = u.get("output_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
            Some((0, c, 0, 0))
        }
        _ => None,
    }
}

fn classify_error(c: u16) -> ErrorType { match c { 429 => ErrorType::RateLimit, 408 => ErrorType::Timeout, s if s >= 500 => ErrorType::ServerError, _ => ErrorType::ConnectionError } }
fn should_retry(e: &ErrorType, r: &[ErrorType]) -> bool { r.contains(e) }

/// 同协议非流式响应优先透传：只把 model 改回虚拟模型名，保留 thinking / 未知字段
fn passthrough_response(mut raw: serde_json::Value, virtual_model: &str) -> HttpResponse {
    if let Some(obj) = raw.as_object_mut() {
        if obj.contains_key("model") {
            obj.insert("model".into(), serde_json::Value::String(virtual_model.to_string()));
        }
    }
    HttpResponse::Ok().content_type("application/json").body(raw.to_string())
}
