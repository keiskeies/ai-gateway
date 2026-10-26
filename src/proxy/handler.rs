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

/// 入站元数据：路径始终记录；请求头只在采集开启时带值。
#[derive(Clone, Default)]
pub struct InboundMeta {
    pub path: Option<String>,
    pub headers: Option<CapturedHeaders>,
}

#[derive(Clone)]
pub struct CapturedHeaders {
    pub value: serde_json::Value,
    pub bytes: i64,
    pub truncated: bool,
}

/// 只收这些头。Authorization / x-api-key / Cookie 等永远不在名单里，
/// 这比“先全存再打码”可靠：新出现的敏感头不会被漏掉。
const CAPTURED_HEADER_NAMES: [&str; 8] = [
    "content-type",
    "accept",
    "user-agent",
    "x-request-id",
    "anthropic-version",
    "anthropic-beta",
    "openai-organization",
    "openai-beta",
];
const MAX_CAPTURED_HEADERS_BYTES: usize = 8 * 1024;

/// 响应正文上限：流式 SSE 也只留前 1MB，只用于排查。
/// 超限标记 `truncated`，详情页会解析已收到的部分并在顶部提示不完整。
pub const MAX_CAPTURED_RESPONSE_BYTES: usize = 1024 * 1024;

fn inbound_meta(req: &HttpRequest, capture_headers: bool) -> InboundMeta {
    let path = Some(req.path().to_string());
    if !capture_headers {
        return InboundMeta { path, headers: None };
    }
    let mut map = serde_json::Map::new();
    let mut bytes = 0usize;
    let mut truncated = false;
    for name in CAPTURED_HEADER_NAMES {
        let mut values = Vec::new();
        for value in req.headers().get_all(name) {
            let text = String::from_utf8_lossy(value.as_bytes()).to_string();
            bytes += name.len() + text.len();
            if bytes > MAX_CAPTURED_HEADERS_BYTES {
                truncated = true;
                break;
            }
            values.push(serde_json::Value::String(text));
        }
        if truncated {
            break;
        }
        match values.len() {
            0 => {}
            1 => { map.insert(name.to_string(), values.remove(0)); }
            _ => { map.insert(name.to_string(), serde_json::Value::Array(values)); }
        }
    }
    InboundMeta {
        path,
        headers: Some(CapturedHeaders {
            value: serde_json::Value::Object(map),
            bytes: bytes.min(MAX_CAPTURED_HEADERS_BYTES) as i64,
            truncated,
        }),
    }
}

fn capture_headers(state: &Arc<ProxyState>, request_id: &str, meta: &InboundMeta) {
    let Some(headers) = &meta.headers else { return };
    state.stats_writer.record_headers(crate::models::stats::RequestHeaders {
        request_id: request_id.to_string(),
        headers: headers.value.to_string(),
        bytes: headers.bytes,
        truncated: headers.truncated,
        created_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// 转发地址打码：query 里常见 `?key=` / `?token=`，以及个别平台把密钥拼进 URL。
/// 保留其余参数（Azure 的 api-version、Anthropic 的 beta 等）便于排查。
fn sanitize_forward_url(raw: &str, secret: &str) -> String {
    if !raw.contains("://") {
        return "[invalid url]".to_string();
    }
    let mut redacted = raw.to_string();
    if !secret.is_empty() {
        redacted = redacted.replace(secret, "redacted");
    }
    let Some((base, query)) = redacted.split_once('?') else { return redacted };
    let (query, fragment) = match query.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (query, None),
    };
    const SENSITIVE: [&str; 8] = ["key", "api_key", "apikey", "token", "access_token", "sig", "signature", "auth"];
    let mut pairs = Vec::new();
    for pair in query.split('&') {
        let (name, _value) = match pair.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (pair, None),
        };
        let lowered = name.to_ascii_lowercase();
        if SENSITIVE.contains(&lowered.as_str()) {
            pairs.push(format!("{}=redacted", name));
        } else {
            pairs.push(pair.to_string());
        }
    }
    let mut out = format!("{}?{}", base, pairs.join("&"));
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn upstream_protocol(platform_type: &crate::models::platform::PlatformType) -> &'static str {
    match platform_type {
        crate::models::platform::PlatformType::Anthropic => "anthropic",
        _ => "openai",
    }
}

/// 非流式响应正文采集。流式走 StreamCapture，二者共用同一个上限。
fn capture_response_body(
    state: &Arc<ProxyState>,
    request_id: &str,
    protocol: &str,
    status_code: i64,
    content_type: Option<String>,
    bytes: &[u8],
) {
    let capture = state.config.read().defaults.capture_response_bodies;
    if !capture {
        return;
    }
    let truncated = bytes.len() > MAX_CAPTURED_RESPONSE_BYTES;
    let slice = &bytes[..bytes.len().min(MAX_CAPTURED_RESPONSE_BYTES)];
    let body = String::from_utf8_lossy(slice).to_string();
    state.stats_writer.record_response_body(crate::models::stats::ResponseBody {
        request_id: request_id.to_string(),
        protocol: protocol.to_string(),
        status_code,
        content_type,
        bytes: bytes.len() as i64,
        body,
        truncated,
        created_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// 流式响应采集：只追加原始上游字节，不参与转换；超过上限后停止追加。
pub struct StreamCapture {
    writer: StatsWriter,
    request_id: String,
    protocol: String,
    status_code: i64,
    content_type: Option<String>,
    buf: Vec<u8>,
    /// 收到过的总字节数（不受截断影响，详情页用它说明原始大小）
    total_bytes: usize,
    truncated: bool,
    sent: bool,
}

impl StreamCapture {
    fn new(
        writer: &StatsWriter,
        request_id: &str,
        protocol: &str,
        status_code: i64,
        content_type: Option<String>,
    ) -> Self {
        Self {
            writer: writer.clone(),
            request_id: request_id.to_string(),
            protocol: protocol.to_string(),
            status_code,
            content_type,
            buf: Vec::new(),
            total_bytes: 0,
            truncated: false,
            sent: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.total_bytes += bytes.len();
        if self.buf.len() >= MAX_CAPTURED_RESPONSE_BYTES {
            self.truncated = true;
            return;
        }
        let remaining = MAX_CAPTURED_RESPONSE_BYTES - self.buf.len();
        if bytes.len() > remaining {
            self.buf.extend_from_slice(&bytes[..remaining]);
            self.truncated = true;
        } else {
            self.buf.extend_from_slice(bytes);
        }
    }

    fn flush(&mut self) {
        if self.sent {
            return;
        }
        self.sent = true;
        let body = String::from_utf8_lossy(&self.buf).to_string();
        self.writer.record_response_body(crate::models::stats::ResponseBody {
            request_id: self.request_id.clone(),
            protocol: self.protocol.clone(),
            status_code: self.status_code,
            content_type: self.content_type.clone(),
            bytes: self.total_bytes as i64,
            body,
            truncated: self.truncated,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }
}

/// 一次成功预留的积分；失败分支必须调用 release，成功分支则保留扣费。
struct CreditReservation {
    platform_id: String,
    window: Option<String>,
    price: f64,
}

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
    let meta = inbound_meta(&req, state.config.read().defaults.capture_request_bodies);

    // 解析会吃掉这份 JSON，所以要在交给解析器之前留一份原样正文（采集开着才用得上）
    let raw = body.0.clone();
    let unified = openai::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::OpenAI, auth_key, Some(raw), None, meta).await
}

/// Global Anthropic-compatible endpoint: POST /v1/messages
pub async fn anthropic_messages(
    state: web::Data<Arc<ProxyState>>, req: HttpRequest, body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let auth = extract_auth(&req);
    let auth_key = validate_auth_cached(&state.cache, &auth)?;
    let meta = inbound_meta(&req, state.config.read().defaults.capture_request_bodies);

    let raw = body.0.clone();
    let unified = anthropic::parse_request(body.into_inner()).map_err(AppError::BadRequest)?;
    handle_request(&state, unified, crate::models::proxy::Protocol::Anthropic, auth_key, Some(raw), None, meta).await
}

/// 正文采集上限：超过时保留前缀并标记 `truncated`，详情仍可阅读；截断的正文不能重放。
pub const MAX_CAPTURED_REQUEST_BYTES: usize = 512 * 1024;

/// 按 UTF-8 字符边界截断，避免把多字节字符切成半个导致详情里出现乱码。
fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

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
    let bytes = text.len();
    let truncated = bytes > MAX_CAPTURED_REQUEST_BYTES;
    if truncated {
        tracing::debug!(
            "请求正文 {} 字节超过上限，只保存前 {} 字节（request_id={}）",
            bytes, MAX_CAPTURED_REQUEST_BYTES, request_id
        );
    }
    let stored = truncate_utf8(&text, MAX_CAPTURED_REQUEST_BYTES);
    state.stats_writer.record_body(crate::models::stats::RequestBody {
        request_id: request_id.to_string(),
        protocol: protocol.to_string(),
        body: stored.to_string(),
        bytes: bytes as i64,
        truncated,
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
    handle_request(state, unified, protocol, None, Some(value), Some(stat_id), InboundMeta::default()).await
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
    path: Option<&str>,
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
        // 成本/积分由调用方在拿到 token 或确认扣费后填写；这里还不知道用量
        cost: None,
        cache_savings: None,
        credits: None,
        path: path.map(str::to_string),
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
    forward_url: Option<String>,
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
        forward_url,
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

fn credit_ok(
    rb: &crate::cache::ResolvedBackend,
    map: Option<&std::collections::HashMap<String, crate::db::credit::CreditState>>,
) -> bool {
    let Some(budget) = rb.platform.credit_budget else { return true };
    if !budget.is_finite() || budget <= 0.0 {
        return true;
    }
    let price = rb.backend.credit_price.unwrap_or(0.0);
    if !price.is_finite() || price <= 0.0 {
        return true;
    }
    let Some(window) = rb.platform.credit_window.as_deref() else { return true };
    let Some(map) = map else { return true };
    let spent = crate::db::credit::effective_spent(map, &rb.platform.id, window);
    budget - spent + f64::EPSILON >= price
}

fn credit_exhausted_message(route: &crate::cache::ResolvedRoute) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut parts = Vec::new();
    for rb in &route.backends {
        if rb.platform.credit_budget.is_none() || !seen.insert(rb.platform.id.clone()) {
            continue;
        }
        let reset = rb.platform.credit_window.as_deref()
            .and_then(crate::db::credit::next_reset_at)
            .unwrap_or_else(|| "unknown".to_string());
        parts.push(format!("{} resets at {}", rb.platform.name, reset));
    }
    if parts.is_empty() {
        "upstream credits exhausted".to_string()
    } else {
        format!("upstream credits exhausted ({})", parts.join("; "))
    }
}

async fn load_credit_map(
    state: &Arc<ProxyState>,
) -> Result<std::collections::HashMap<String, crate::db::credit::CreditState>, AppError> {
    let db = state.db.clone();
    web::block(move || crate::db::credit::state_map(&db))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn reserve_credit(
    state: &Arc<ProxyState>,
    resolved: &crate::cache::ResolvedBackend,
    request_id: &str,
) -> Result<crate::db::credit::CreditReserve, AppError> {
    let db = state.db.clone();
    let platform_id = resolved.platform.id.clone();
    let budget = resolved.platform.credit_budget;
    let window = resolved.platform.credit_window.clone();
    let price = resolved.backend.credit_price;
    let request_id = request_id.to_string();
    let model_id = resolved.backend.model_id.clone();
    web::block(move || {
        crate::db::credit::try_reserve(&db, &platform_id, budget, window.as_deref(), price, &request_id, &model_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn release_credit(
    state: &Arc<ProxyState>,
    reservation: CreditReservation,
    request_id: &str,
    model_id: &str,
) {
    let db = state.db.clone();
    let platform_id = reservation.platform_id;
    let price = reservation.price;
    let window = reservation.window;
    let request_id = request_id.to_string();
    let model_id = model_id.to_string();
    match web::block(move || {
        crate::db::credit::release(&db, &platform_id, window.as_deref(), price, &request_id, &model_id)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!("释放积分预留失败: {}", e),
        Err(e) => tracing::warn!("释放积分预留任务失败: {}", e),
    }
}

async fn handle_request(
    state: &Arc<ProxyState>,
    unified: UnifiedRequest,
    response_protocol: crate::models::proxy::Protocol,
    auth_key: Option<crate::models::api_key::StoredApiKey>,
    raw_body: Option<serde_json::Value>,
    replayed_from: Option<i64>,
    meta: InboundMeta,
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
    // 正文/请求头按这个 request_id 挂到那条统计行上；放在路由解析之后，是为了不给
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
        capture_headers(state, &request_id, &meta);
    }
    let path = meta.path.as_deref();
    let capture_response = state.config.read().defaults.capture_response_bodies;
    // 分时价格用请求开始时刻判定；长流跨过高峰边界也不换价。
    let pricing_at = chrono::Local::now();
    // 只有真的会按次计费的平台才查状态；没有积分配置的路由零额外查询。
    let credit_enabled = resolved_route.backends.iter().any(|rb| {
        rb.platform.credit_budget.is_some() && rb.backend.credit_price.unwrap_or(0.0) > 0.0
    });

    for attempt in 0..=max_retries {
        // 每次尝试重新取快照：同一请求内的失败释放会改变余额，
        // 用开头的旧快照会把刚释放的平台误判为耗尽。
        let credit_map = if credit_enabled { Some(load_credit_map(state).await?) } else { None };
        let active: Vec<&crate::cache::ResolvedBackend> = resolved_route.backends.iter()
            .filter(|rb| rb.backend.status == crate::models::route::BackendStatus::Active)
            .collect();
        let mut candidates: Vec<crate::models::route::Backend> = active.iter()
            .filter(|rb| credit_ok(rb, credit_map.as_ref()))
            .map(|rb| rb.backend.clone())
            .collect();
        if candidates.is_empty() {
            if active.is_empty() {
                return Err(AppError::Internal("No available backend".to_string()));
            }
            return Err(AppError::TooManyRequests(credit_exhausted_message(&resolved_route)));
        }

        // 选后端 + 预留积分。预留失败（被并发抢光）就把该后端从本轮候选里去掉再选。
        let (backend, resolved, charge, mut reservation) = loop {
            let Some(selected) = state.selector.select(
                &resolved_route.route_id,
                &candidates,
                &resolved_route.lb_strategy,
            ) else {
                return Err(AppError::TooManyRequests(credit_exhausted_message(&resolved_route)));
            };
            let selected = selected.clone();
            let resolved = resolved_route.backends.iter()
                .find(|rb| rb.backend.id == selected.id)
                .cloned()
                .ok_or_else(|| AppError::Internal("Backend not found in cache".to_string()))?;
            let needs_reserve = resolved.platform.credit_budget.is_some()
                && resolved.backend.credit_price.unwrap_or(0.0) > 0.0;
            let reserve = if needs_reserve {
                reserve_credit(state, &resolved, &request_id).await?
            } else if resolved.platform.credit_budget.is_some() {
                crate::db::credit::CreditReserve::Free
            } else {
                crate::db::credit::CreditReserve::NotMetered
            };
            match reserve {
                crate::db::credit::CreditReserve::NotMetered => break (selected, resolved, None, None),
                crate::db::credit::CreditReserve::Free => break (selected, resolved, Some(0.0), None),
                crate::db::credit::CreditReserve::Reserved { charged } => {
                    let reservation = CreditReservation {
                        platform_id: resolved.platform.id.clone(),
                        window: resolved.platform.credit_window.clone(),
                        price: charged,
                    };
                    break (selected, resolved, Some(charged), Some(reservation));
                }
                crate::db::credit::CreditReserve::Exhausted { .. } => {
                    candidates.retain(|b| b.id != selected.id);
                    if candidates.is_empty() {
                        return Err(AppError::TooManyRequests(credit_exhausted_message(&resolved_route)));
                    }
                }
            }
        };

        state.selector.inc_connection(&resolved_route.route_id, &backend.id);
        let start = Instant::now();
        let started_at = chrono::Utc::now().to_rfc3339();
        let attempt_no = attempt as i64 + 1;

        let (forward_body, forward_url_raw) = build_forward_request(&unified, &resolved.platform, &resolved.backend, resolved_route.default_max_tokens);
        let forward_url = sanitize_forward_url(&forward_url_raw, &resolved.platform.api_key);

        let mut req_builder = state.http_client.post(&forward_url_raw)
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
                let content_type = resp.headers().get("content-type")
                    .and_then(|v| v.to_str().ok()).map(str::to_string);
                if status.is_success() {
                    // 拿到响应头即算本次尝试成功；积分预留到此确认，不再释放。
                    state.stats_writer.record_attempt(make_attempt(
                        &request_id,
                        attempt_no,
                        &backend,
                        Some(status.as_u16() as i64),
                        None,
                        None,
                        latency_ms,
                        started_at.clone(),
                        Some(forward_url.clone()),
                    ));
                    if is_stream {
                        // 流式：统计由 handle_stream / handle_stream_convert 在流结束时记录一次（含 SSE usage token）
                        let mut base = make_stat(
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
                            path,
                        );
                        base.credits = charge;
                        let client_anthropic = matches!(response_protocol, crate::models::proxy::Protocol::Anthropic);
                        let upstream_anthropic = matches!(resolved.platform.platform_type, crate::models::platform::PlatformType::Anthropic);
                        let capture = if capture_response {
                            Some(StreamCapture::new(
                                &state.stats_writer,
                                &request_id,
                                upstream_protocol(&resolved.platform.platform_type),
                                status.as_u16() as i64,
                                content_type,
                            ))
                        } else {
                            None
                        };
                        if client_anthropic == upstream_anthropic {
                            return Ok(handle_stream(resp, &state.stats_writer, base, backend.pricing_snapshot(), start, pricing_at, capture));
                        }
                        tracing::info!("跨协议流式转换：客户端={} 上游={}",
                            if client_anthropic { "anthropic" } else { "openai" },
                            if upstream_anthropic { "anthropic" } else { "openai" });
                        return Ok(handle_stream_convert(resp, &state.stats_writer, base, backend.pricing_snapshot(), client_anthropic, &virtual_model, &backend.id, &backend.model_id, start, pricing_at, capture));
                    }
                    let body = resp.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    capture_response_body(
                        state,
                        &request_id,
                        upstream_protocol(&resolved.platform.platform_type),
                        status.as_u16() as i64,
                        content_type,
                        &body,
                    );
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
                        path,
                    );
                    stat.cache_read_tokens = cache_read;
                    stat.cache_write_tokens = cache_write;
                    // 一次快照同时算出成本与缓存价差：两者必须用同一档位（高峰/空闲）
                    let pricing = backend.pricing_snapshot();
                    stat.cost = pricing.as_ref().and_then(|pricing| {
                        pricing.cost(token_input, token_output, cache_read, cache_write, pricing_at)
                    });
                    stat.cache_savings = pricing
                        .as_ref()
                        .and_then(|pricing| pricing.cache_savings(cache_read, pricing_at));
                    stat.credits = charge;
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
                    // 上游业务失败：先释放预留，再落尝试轨迹，最后判断是否重试
                    if let Some(r) = reservation.take() {
                        release_credit(state, r, &request_id, &backend.model_id).await;
                    }
                    let et = format!("{:?}", classify_error(status.as_u16()));
                    last_error = Some(format!("upstream {} ({})", status.as_u16(), et));
                    let eb = resp.text().await.unwrap_or_default();
                    capture_response_body(
                        state,
                        &request_id,
                        upstream_protocol(&resolved.platform.platform_type),
                        status.as_u16() as i64,
                        content_type,
                        eb.as_bytes(),
                    );
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
                        Some(forward_url.clone()),
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
                        path,
                    ));
                    return Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)).body(eb));
                }
            }
            Err(e) => {
                // 连接错误：先释放预留，再逐次记录轨迹；统计只在最终结果落一条
                if let Some(r) = reservation.take() {
                    release_credit(state, r, &request_id, &backend.model_id).await;
                }
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
                    Some(forward_url.clone()),
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
                    path,
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
    capture: Option<StreamCapture>,
}

/// body 被 drop 时兜底记录统计（覆盖客户端中途断开的情况），record_once 用 CAS 保证恰好一次
impl Drop for StreamPassthroughBody {
    fn drop(&mut self) {
        self.stats.record_once();
        if let Some(capture) = &mut self.capture {
            capture.flush();
        }
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
    /// 每条请求的token/价格快照：缓存 token 在流结束时才知道，所以价格先随上下文带进来
    pricing: Option<crate::models::pricing::Pricing>,
    pricing_at: chrono::DateTime<chrono::Local>,
    writer: StatsWriter,
}

impl StreamStatCtx {
    /// start 必须是请求发出时刻（与 latency_ms 同源），首字时长以此为基准
    fn new(
        base: RequestStat,
        writer: &StatsWriter,
        start: Instant,
        pricing: Option<crate::models::pricing::Pricing>,
        pricing_at: chrono::DateTime<chrono::Local>,
    ) -> Self {
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
            pricing,
            pricing_at,
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
        let cr = self.cache_read.load(Ordering::SeqCst);
        let cw = self.cache_write.load(Ordering::SeqCst);
        stat.token_input = Some(self.token_in.load(Ordering::SeqCst));
        stat.token_output = Some(self.token_out.load(Ordering::SeqCst));
        stat.cost = self.pricing.as_ref().and_then(|pricing| {
            pricing.cost(stat.token_input, stat.token_output, Some(cr), Some(cw), self.pricing_at)
        });
        stat.cache_savings = self
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.cache_savings(Some(cr), self.pricing_at));
        // 缓存命中仅在流内出现过 usage 时非 0
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
                if let Some(capture) = &mut self.capture {
                    capture.flush();
                }
                self.stats.record_once();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(bytes))) => {
                // 收到上游第一个数据 chunk：记录首字时长（CAS 幂等）
                self.stats.mark_first_token();
                if let Some(capture) = &mut self.capture {
                    capture.push(&bytes);
                }
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
                if let Some(capture) = &mut self.capture {
                    capture.flush();
                }
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
    pricing: Option<crate::models::pricing::Pricing>,
    start: Instant,
    pricing_at: chrono::DateTime<chrono::Local>,
    capture: Option<StreamCapture>,
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

    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start, pricing, pricing_at));
    let body = StreamPassthroughBody {
        inner: Box::pin(resp.bytes_stream()),
        buffer: String::new(),
        stats,
        capture,
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
    capture: Option<StreamCapture>,
}

impl Drop for StreamConvertBody {
    fn drop(&mut self) {
        // 客户端中途断开时兜底记录（record_once CAS 保证幂等）
        self.stats.record_once();
        if let Some(capture) = &mut self.capture {
            capture.flush();
        }
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
                    if let Some(capture) = &mut self.capture {
                        capture.flush();
                    }
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
                    // 采的是上游原始字节，必须在转换器之前追加
                    if let Some(capture) = &mut self.capture {
                        capture.push(&bytes);
                    }
                    let out = self.converter.push_chunk(&bytes);
                    if out.is_empty() { continue; } // 事件未完整，继续读上游
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Err(e))) => {
                    // 上游流错误：按客户端协议发错误帧并正常收尾（SSE 已开始无法改状态码）
                    tracing::warn!("跨协议流式上游错误: {}", e);
                    self.stats.failed.store(true, Ordering::SeqCst);
                    if let Some(capture) = &mut self.capture {
                        capture.flush();
                    }
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
    pricing: Option<crate::models::pricing::Pricing>,
    client_anthropic: bool,
    model: &str,
    backend_id: &str,
    upstream_model: &str,
    start: Instant,
    pricing_at: chrono::DateTime<chrono::Local>,
    capture: Option<StreamCapture>,
) -> HttpResponse {
    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start, pricing, pricing_at));
    let body = StreamConvertBody {
        inner: Box::pin(resp.bytes_stream()),
        converter: crate::proxy::stream_convert::StreamConverter::new(
            client_anthropic,
            model,
            backend_id,
            upstream_model,
        ),
        stats,
        capture,
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

#[cfg(test)]
mod tests {
    use super::{sanitize_forward_url, truncate_utf8};

    #[test]
    fn utf8_truncation_never_splits_a_character() {
        let text = "ab汉字cd";
        // "ab" 后正好是 3 字节的汉字；上限落在字符中间时回退到边界
        assert_eq!(truncate_utf8(text, 3), "ab");
        assert_eq!(truncate_utf8(text, 4), "ab");
        assert_eq!(truncate_utf8(text, 5), "ab汉");
        assert_eq!(truncate_utf8(text, 99), text);
    }

    #[test]
    fn forward_url_redacts_query_credentials_and_secrets() {
        assert_eq!(
            sanitize_forward_url("https://api.example.com/v1/chat/completions?key=sk-123&api-version=1", "sk-123"),
            "https://api.example.com/v1/chat/completions?key=redacted&api-version=1"
        );
        assert_eq!(
            sanitize_forward_url("https://api.example.com/v1?token=abc&sig=xyz&q=ok#frag", ""),
            "https://api.example.com/v1?token=redacted&sig=redacted&q=ok#frag"
        );
        // 自定义 query 名也可能塞了密钥：只要值等于平台 Key 就替换
        assert_eq!(
            sanitize_forward_url("https://api.example.com/v1?auth=sk-secret", "sk-secret"),
            "https://api.example.com/v1?auth=redacted"
        );
        assert_eq!(sanitize_forward_url("not a url", ""), "[invalid url]");
    }
}
