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

        let (forward_body, forward_url) = build_forward_request(&unified, &resolved.platform, &resolved.backend, resolved_route.default_max_tokens);

        let mut req_builder = state.http_client.post(&forward_url)
            // 流式长生成（推理模型/长输出）远超 request_timeout，单独放宽到 stream_timeout
            .timeout(std::time::Duration::from_secs(if is_stream {
                state.config.defaults.stream_timeout_secs
            } else {
                state.config.defaults.request_timeout_secs
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
                        );
                        // 协议匹配 → 透传；跨协议 → 实时转换（Anthropic 事件流 ↔ OpenAI chunk 流）
                        let client_anthropic = matches!(response_protocol, crate::models::proxy::Protocol::Anthropic);
                        let upstream_anthropic = matches!(resolved.platform.platform_type, crate::models::platform::PlatformType::Anthropic);
                        if client_anthropic == upstream_anthropic {
                            return Ok(handle_stream(resp, &state.stats_writer, base, start));
                        }
                        tracing::info!("跨协议流式转换：客户端={} 上游={}",
                            if client_anthropic { "anthropic" } else { "openai" },
                            if upstream_anthropic { "anthropic" } else { "openai" });
                        return Ok(handle_stream_convert(resp, &state.stats_writer, base, client_anthropic, &virtual_model, start));
                    }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let raw: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                    // 解析：按上游平台协议解析为统一格式
                    let parsed = match resolved.platform.platform_type {
                        crate::models::platform::PlatformType::Anthropic => match anthropic::parse_response(raw.clone()) {
                            Ok(ur) => ur,
                            Err(e) => {
                                // 解析失败兜底：透传原始响应体，避免上游小差异导致整个网关 500
                                tracing::warn!("解析 Anthropic 响应失败（透传原始响应）: {}", e);
                                return Ok(HttpResponse::Ok().json(raw.clone()));
                            }
                        },
                        _ => match openai::parse_response(raw.clone()) {
                            Ok(ur) => ur,
                            Err(e) => {
                                tracing::warn!("解析 OpenAI 响应失败（透传原始响应）: {}", e);
                                return Ok(HttpResponse::Ok().json(raw.clone()));
                            }
                        },
                    };
                    let token_input = parsed.usage.as_ref().map(|u| u.prompt_tokens as i64);
                    let token_output = parsed.usage.as_ref().map(|u| u.completion_tokens as i64);
                    // 缓存命中（OpenAI cached_tokens / Anthropic cache_read，已在协议层归一）
                    let cache_read = parsed.usage.as_ref().and_then(|u| u.cache_read_tokens).map(|v| v as i64);
                    let cache_write = parsed.usage.as_ref().and_then(|u| u.cache_write_tokens).map(|v| v as i64);

                    // 生成：按客户端请求的协议格式输出
                    let cr = match response_protocol {
                        crate::models::proxy::Protocol::Anthropic => anthropic::to_response(&parsed),
                        _ => openai::to_response(&parsed),
                    };

                    // 成功路径：恰好记录一次
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
                    );
                    stat.cache_read_tokens = cache_read;
                    stat.cache_write_tokens = cache_write;
                    state.stats_writer.record(stat);
                    return Ok(HttpResponse::Ok().json(cr));
                } else {
                    // 上游业务失败：先判断是否重试，不在此处记录（避免重试重复计数）
                    let et = format!("{:?}", classify_error(status.as_u16()));
                    last_error = Some(format!("upstream {} ({})", status.as_u16(), et));
                    if should_retry(&classify_error(status.as_u16()), &resolved_route.retry_policy.retry_on_error) && attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    // 最终失败：记录一条统计并透传原始响应体
                    let eb = resp.text().await.unwrap_or_default();
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
                    ));
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                // 连接错误：不在此处记录，由循环结束后统一记录最终结果（避免重试重复计数）
                last_error = Some(format!("Connection: {}", e));
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
    use crate::models::route::UnknownParams;
    // 未知参数策略：Auto 时按平台自动决定（Anthropic 严格校验未知字段 → 丢弃；OpenAI 兼容上游普遍忽略 → 透传）
    let unknown = match &backend.unknown_params {
        UnknownParams::Auto => match platform.platform_type {
            crate::models::platform::PlatformType::Anthropic => UnknownParams::Drop,
            _ => UnknownParams::Passthrough,
        },
        explicit => explicit.clone(),
    };
    let (mut body, url) = match platform.platform_type {
        crate::models::platform::PlatformType::Anthropic => {
            (anthropic::to_request(unified, &backend.model_id, default_max_tokens, unknown), format!("{}/v1/messages", platform.base_url.trim_end_matches('/')))
        }
        _ => {
            (openai::to_request(unified, &backend.model_id, unknown), format!("{}/chat/completions", platform.base_url.trim_end_matches('/')))
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
    writer: StatsWriter,
}

impl StreamStatCtx {
    /// start 必须是请求发出时刻（与 latency_ms 同源），首字时长以此为基准
    fn new(base: RequestStat, writer: &StatsWriter, start: Instant) -> Self {
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

    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start));
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
    client_anthropic: bool,
    model: &str,
    start: Instant,
) -> HttpResponse {
    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start));
    let body = StreamConvertBody {
        inner: Box::pin(resp.bytes_stream()),
        converter: crate::proxy::stream_convert::StreamConverter::new(client_anthropic, model),
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
