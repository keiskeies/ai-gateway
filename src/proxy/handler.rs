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
    // 用乘数后的实际价格判断余额：否则「倍率 0.5 的模型」会被按原价误判为余额不足
    let price = rb.backend.effective_credit_price().unwrap_or(0.0);
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
    let price = resolved.backend.effective_credit_price();
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
        rb.platform.credit_budget.is_some() && rb.backend.effective_credit_price().unwrap_or(0.0) > 0.0
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
            // 冷却中的后端不参与本轮：上游刚报过配额耗尽/风控，立刻再打只会延长封禁
            .filter(|rb| !is_cooling_down(&rb.backend.id))
            .map(|rb| rb.backend.clone())
            .collect();
        if candidates.is_empty() {
            if active.is_empty() {
                return Err(AppError::Internal("No available backend".to_string()));
            }
            // 全部在冷却：区分「额度耗尽」与「上游风控」，给出可操作的提示
            let all_cooling = active.iter().all(|rb| is_cooling_down(&rb.backend.id));
            if all_cooling {
                return Err(AppError::TooManyRequests(
                    "所有上游后端均在冷却中（配额耗尽或风控），请稍后重试".to_string(),
                ));
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
                && resolved.backend.effective_credit_price().unwrap_or(0.0) > 0.0;
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

        // 三条路径的判定：标准协议走原来的 build_forward_request，
        // 私有协议（Trae / WorkBuddy）由 upstream 适配层给出完整转发计划。
        let adapter = crate::models::adapter::PlatformAdapter::resolve(
            resolved.platform.adapter.as_deref(),
            &resolved.platform.platform_type,
        );
        let adapter_credential = resolved.credential.as_ref().map(|secret| {
            crate::proxy::upstream::AdapterCredential {
                access_token: secret.access_token.as_str(),
                refresh_token: secret.refresh_token.as_deref(),
                uid: secret.uid.as_deref(),
                enterprise_id: secret.enterprise_id.as_deref(),
                domain: secret.domain.as_deref(),
                device_id: secret.device_id.as_deref(),
                machine_id: secret.machine_id.as_deref(),
            }
        });
        // 打开路由开关时，每次上游尝试都在整段 prompt 最前面插一串 16~32 位随机数字。
        // 注入面按目标协议选：Anthropic 与私有适配器读 system 字段，OpenAI 兼容上游只渲染 messages。
        let nonce_surface = if adapter.is_proprietary()
            || resolved.platform.platform_type == crate::models::platform::PlatformType::Anthropic
        {
            crate::protocol::nonce::Surface::SystemField
        } else {
            crate::protocol::nonce::Surface::Messages
        };
        let mut nonced_unified;
        let unified_ref: &UnifiedRequest = if resolved_route.request_nonce {
            nonced_unified = unified.clone();
            crate::protocol::nonce::apply(&mut nonced_unified, nonce_surface);
            &nonced_unified
        } else {
            &unified
        };

        let plan = crate::proxy::upstream::build(
            adapter,
            &resolved.platform,
            &resolved.backend,
            unified_ref,
            adapter_credential.as_ref(),
            resolved_route.default_max_tokens,
        )?;

        let (forward_body, forward_url_raw) = match &plan {
            Some(plan) => (plan.body.clone(), plan.url.clone()),
            None => {
                let (body, url) = build_forward_request(unified_ref, &resolved.platform, &resolved.backend, resolved_route.default_max_tokens);
                (body, url)
            }
        };
        let forward_url = sanitize_forward_url(&forward_url_raw, &resolved.platform.api_key);

        let mut req_builder = state.http_client.post(&forward_url_raw)
            // 流式长生成（推理模型/长输出）远超 request_timeout，单独放宽到 stream_timeout
            .timeout(std::time::Duration::from_secs(if is_stream {
                state.config.read().defaults.stream_timeout_secs
            } else {
                state.config.read().defaults.request_timeout_secs
            }))
            .header("Content-Type", "application/json");

        // 私有协议：适配器给的请求头里已含鉴权，直接照发
        if let Some(plan) = &plan {
            for (name, value) in &plan.headers {
                req_builder = req_builder.header(name.as_str(), value.as_str());
            }
        } else if !resolved.platform.api_key.is_empty() {
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

        // 私有协议上游总是回 SSE（即使客户端要非流式）。这个事实由 `private_protocol_kind`
        // 在响应分支里判断——那里同时决定「透传 SSE」还是「聚合后再返回 JSON」。

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
                    // 私有协议上游总是回 SSE：即便客户端要非流式，也要走流式路径再聚合，
                    // 否则会把 SSE 正文当 JSON 解析（得到空响应）。
                    let private_protocol = plan.as_ref().map(|p| p.always_sse).unwrap_or(false);
                    if is_stream || private_protocol {
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
                        // 私有协议：先把上游 SSE 归一化成 OpenAI 形态，再决定透传还是转换
                        if let Some(kind) = private_protocol_kind(adapter) {
                            // 关键分支：客户端要非流式时，必须把整条 SSE 读完后聚合成 JSON，
                            // 不能直接把 SSE 透传给一个等 JSON 的客户端。
                            if !is_stream {
                                tracing::info!("私有协议非流式聚合（流式路径内）：adapter={:?}", adapter);
                                return Ok(handle_private_aggregate(
                                    resp,
                                    &state.stats_writer,
                                    base,
                                    backend.pricing_snapshot(),
                                    kind,
                                    client_anthropic,
                                    &virtual_model,
                                    start,
                                    pricing_at,
                                )
                                .await);
                            }
                            tracing::info!(
                                "私有协议流式：adapter={:?} 客户端={}",
                                adapter,
                                if client_anthropic { "anthropic" } else { "openai" }
                            );
                            return Ok(handle_private_stream(
                                resp,
                                &state.stats_writer,
                                base,
                                backend.pricing_snapshot(),
                                kind,
                                client_anthropic,
                                &virtual_model,
                                &backend.id,
                                &backend.model_id,
                                start,
                                pricing_at,
                                capture,
                            ));
                        }
                        if client_anthropic == upstream_anthropic {
                            return Ok(handle_stream(resp, &state.stats_writer, base, backend.pricing_snapshot(), client_anthropic, start, pricing_at, capture));
                        }
                        tracing::info!("跨协议流式转换：客户端={} 上游={}",
                            if client_anthropic { "anthropic" } else { "openai" },
                            if upstream_anthropic { "anthropic" } else { "openai" });
                        return Ok(handle_stream_convert(resp, &state.stats_writer, base, backend.pricing_snapshot(), client_anthropic, &virtual_model, &backend.id, &backend.model_id, start, pricing_at, capture));
                    }
                    // 走到这里说明是标准协议（私有协议已在上面 `is_stream || private_protocol`
                    // 分支里全部处理完，因为它们的 always_sse 恒为 true）。
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
                    let eb = resp.text().await.unwrap_or_default();
                    // 错误分类：私有协议要按上游业务码判（Trae 4008/4010/4015、
                    // WorkBuddy 14018 等），标准协议按 HTTP 状态码判。
                    // 这一步决定「换号 / 冷却 / 直接失败」，走错桶会让单账号封禁
                    // 升级成 IP 段封禁，或让必然失败的请求在每个账号上轮流撞一遍。
                    let failure = classify_upstream_failure(adapter, status.as_u16(), &eb);
                    if failure.should_cooldown() {
                        if let Some(secs) = failure.cooldown_secs() {
                            cooldown_backend(state, &backend.id, secs);
                            tracing::warn!(
                                "上游失败冷却：platform={} backend={} 冷却 {}s（{:?}）",
                                resolved.platform.name, backend.id, secs, failure
                            );
                        }
                        if failure.should_alert() {
                            tracing::error!(
                                "上游风控告警：platform={} —— {}；继续换号会放大封禁，已停止轮转",
                                resolved.platform.name, failure_message(&failure)
                            );
                        }
                    }
                    let et = format!("{:?}", classify_error(status.as_u16()));
                    last_error = Some(format!("upstream {} ({})", status.as_u16(), et));
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
                    // 是否换后端重试：ClientError / RiskControl 一律不重试
                    // （前者换号也没用，后者换号会放大封禁）。
                    let retry_worthwhile = failure.should_try_another_backend()
                        || matches!(
                            failure,
                            crate::proxy::upstream::error_map::UpstreamFailure::UpstreamBusy { .. }
                        );
                    if retry_worthwhile
                        && should_retry(&classify_error(status.as_u16()), &resolved_route.retry_policy.retry_on_error)
                        && attempt < max_retries
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(resolved_route.retry_policy.backoff_ms * 2u64.pow(attempt as u32))).await;
                        continue;
                    }
                    // 最终失败：记录一条统计，并对客户端回映射后的状态码。
                    // Agnes 的 402 要转成 429（客户端只认 429 是「限流/额度」）。
                    let client_status = failure.client_status();
                    state.stats_writer.record(make_stat(
                        &resolved_route.proxy_id,
                        &resolved_route.route_id,
                        &backend.id,
                        client_status as i32,
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
                    return Ok(HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(client_status)
                            .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
                    )
                    .body(eb));
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

/// 同协议透传的收尾看护。
///
/// 为什么需要：同协议流是**逐字节透传**上游 SSE，不像跨协议转换器和私有协议归一化器那样
/// 会补发收尾帧。上游如果在生成中断流（我们实测 600 条流式样本里有 2 条），客户端收到的
/// 是一条既没有 `finish_reason` 也没有 `[DONE]` 的事件流，OpenAI SDK 侧表现为
/// `Stream ended without finish_reason` —— 报错信息指向网关，实际根因在上游。
///
/// 这里只做旁路观察 + 在流结束时补帧，不改动正常流的任何一个字节。
#[derive(Default)]
struct PassthroughTracker {
    /// 客户端是 Anthropic 协议（/v1/messages）：收尾语义换成 message_delta + message_stop
    anthropic: bool,
    /// 出现过非空 finish_reason（OpenAI）或 stop_reason（Anthropic）
    saw_finish_reason: bool,
    /// 见过上游的收尾哨兵：OpenAI 的 `[DONE]` / Anthropic 的 `message_stop`
    saw_done: bool,
    /// 出现过 tool_calls / tool_use：决定补发的 finish_reason 写 tool_calls 还是 stop
    saw_tool_call: bool,
    /// 见过的 usage，收尾帧要带回去（Anthropic 的 message_delta 必须带 output_tokens）
    usage: (i64, i64, i64, i64),
    /// 从首帧记下的 id/model/created，补发的 chunk 沿用它们以免客户端对不上
    chunk_id: Option<String>,
    chunk_model: Option<String>,
    chunk_created: Option<i64>,
    /// 已经补过收尾帧（保证只补一次）
    backfilled: bool,
}

impl PassthroughTracker {
    fn new(anthropic: bool) -> Self {
        Self { anthropic, ..Default::default() }
    }

    /// 观察一行 `data:` 的 payload（调用方已剥掉前缀并 trim）
    fn observe(&mut self, payload: &str, usage: Option<(i64, i64, i64, i64)>) {
        if let Some(u) = usage {
            // usage 是累计值，取最大即可（与 StreamStatCtx::absorb_usage 同口径）
            self.usage.0 = self.usage.0.max(u.0);
            self.usage.1 = self.usage.1.max(u.1);
            self.usage.2 = self.usage.2.max(u.2);
            self.usage.3 = self.usage.3.max(u.3);
        }
        if payload == "[DONE]" {
            self.saw_done = true;
            return;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        if self.anthropic {
            match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "message_stop" => self.saw_done = true,
                "message_delta" => {
                    if let Some(sr) = v.pointer("/delta/stop_reason").and_then(|s| s.as_str()) {
                        self.saw_finish_reason = true;
                        if sr == "tool_use" {
                            self.saw_tool_call = true;
                        }
                    }
                }
                "content_block_start" => {
                    if v.pointer("/content_block/type").and_then(|t| t.as_str()) == Some("tool_use") {
                        self.saw_tool_call = true;
                    }
                }
                _ => {}
            }
            return;
        }
        // OpenAI 形态
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    self.saw_finish_reason = true;
                    if fr == "tool_calls" || fr == "function_call" {
                        self.saw_tool_call = true;
                    }
                }
                if choice.pointer("/delta/tool_calls").and_then(|t| t.as_array()).is_some() {
                    self.saw_tool_call = true;
                }
            }
        }
        if self.chunk_id.is_none() {
            if let Some(id) = v.get("id").and_then(|s| s.as_str()) {
                self.chunk_id = Some(id.to_string());
            }
        }
        if self.chunk_model.is_none() {
            if let Some(m) = v.get("model").and_then(|s| s.as_str()) {
                self.chunk_model = Some(m.to_string());
            }
        }
        if self.chunk_created.is_none() {
            self.chunk_created = v.get("created").and_then(|c| c.as_i64());
        }
    }

    /// 补发的 finish_reason：出现 tool_calls 就写 tool_calls（否则客户端不会执行工具，
    /// 与 private_sse 的 effective_finish_reason 同口径），其余一律 stop。
    fn finish_reason(&self) -> &'static str {
        if self.saw_tool_call { "tool_calls" } else { "stop" }
    }

    /// 是否需要补收尾帧
    fn needs_backfill(&self) -> bool {
        !self.saw_done || !self.saw_finish_reason
    }

    /// 生成收尾帧（只补缺失的那部分）。
    ///
    /// 顺序：OpenAI 客户端是 finish_reason 帧 → `[DONE]`；Anthropic 客户端是
    /// message_delta（stop_reason + output_tokens）→ message_stop。
    fn tail(&self, with_ids: bool) -> String {
        let mut out = String::new();
        if self.anthropic {
            // message_delta 必须在 message_stop 之前；上游若已发过 stop_reason 就不重复
            if !self.saw_finish_reason {
                let evt = serde_json::json!({
                    "type": "message_delta",
                    "delta": {"stop_reason": "end_turn", "stop_sequence": serde_json::Value::Null},
                    "usage": {"output_tokens": self.usage.1}
                });
                out.push_str(&format!("event: message_delta\ndata: {evt}\n\n"));
            }
            if !self.saw_done {
                let evt = serde_json::json!({"type": "message_stop"});
                out.push_str(&format!("event: message_stop\ndata: {evt}\n\n"));
            }
            return out;
        }
        if !self.saw_finish_reason {
            let mut chunk = serde_json::json!({
                "id": self.chunk_id.clone().unwrap_or_else(|| format!("chatcmpl-{}", uuid::Uuid::new_v4().simple())),
                "object": "chat.completion.chunk",
                "created": self.chunk_created.unwrap_or_else(|| chrono::Utc::now().timestamp()),
                "choices": [{
                    "index": 0,
                    "delta": {},
                    "finish_reason": self.finish_reason(),
                }],
            });
            if with_ids {
                if let Some(m) = &self.chunk_model {
                    chunk["model"] = serde_json::json!(m);
                }
            }
            out.push_str(&format!("data: {chunk}\n\n"));
        }
        if !self.saw_done {
            out.push_str("data: [DONE]\n\n");
        }
        out
    }

    /// 流结束时的收尾帧：只在缺收尾信息且没补过时产出
    fn backfill(&mut self) -> String {
        if self.backfilled || !self.needs_backfill() {
            return String::new();
        }
        self.backfilled = true;
        self.tail(true)
    }

    /// 这一行是不是上游的终止哨兵（OpenAI 的 `[DONE]` / Anthropic 的 message_stop）。
    ///
    /// 判它是因为两个协议的客户端都在读到哨兵的那一刻收尾：
    /// OpenAI SDK 遇到 `[DONE]` 直接 break，Anthropic SDK 读到 message_stop 就返回累积结果。
    /// 补发的终止帧必须落在哨兵**之前**，否则永远读不到。
    fn is_terminal_sentinel(&self, payload: &str) -> bool {
        if self.anthropic {
            if !payload.starts_with('{') {
                return false;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
                return false;
            };
            return v.get("type").and_then(|t| t.as_str()) == Some("message_stop");
        }
        payload == "[DONE]"
    }

    /// 只补「终止帧」本身，不含 `[DONE]` / `message_stop`。
    ///
    /// 用于上游已经发了 `[DONE]` 但没发 finish_reason 的情形：那一行 `[DONE]`
    /// 随后会原样转发，所以这里不能重复发哨兵，只需要把缺的终止帧插在它前面。
    fn finish_only(&self) -> String {
        if self.saw_finish_reason {
            return String::new();
        }
        if self.anthropic {
            // message_stop 由上游自己给，这里只补 message_delta
            let evt = serde_json::json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": serde_json::Value::Null},
                "usage": {"output_tokens": self.usage.1}
            });
            return format!("event: message_delta\ndata: {evt}\n\n");
        }
        let mut chunk = serde_json::json!({
            "id": self.chunk_id.clone().unwrap_or_else(|| format!("chatcmpl-{}", uuid::Uuid::new_v4().simple())),
            "object": "chat.completion.chunk",
            "created": self.chunk_created.unwrap_or_else(|| chrono::Utc::now().timestamp()),
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": self.finish_reason(),
            }],
        });
        if let Some(m) = &self.chunk_model {
            chunk["model"] = serde_json::json!(m);
        }
        format!("data: {chunk}\n\n")
    }

    /// 错误帧之后的收尾帧：无论 `backfilled` 状态如何都产出一份，
    /// 保证「错误帧 → 终止帧 → [DONE]」序列完整（否则客户端照样报
    /// `Stream ended without finish_reason`，错误帧在这条链路上反而不可见）。
    fn error_tail(&self) -> String {
        self.tail(true)
    }
}

/// 流式响应透传 body：透传 SSE 给客户端，同时旁路解析流内 usage 累积 token。
/// 统计恰好记录一次：流正常结束、上游出错、或客户端断开（body drop）时都触发。
struct StreamPassthroughBody {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>,
    /// 未凑满一行的残留字节。必须是字节缓冲而不是 String：
    /// 一个 chunk 可能把一个多字节字符切成两半，用 String::from_utf8_lossy
    /// 再做拼接会把汉字写坏，而这条路径是原样透传的。
    buffer: Vec<u8>,
    stats: Arc<StreamStatCtx>,
    capture: Option<StreamCapture>,
    /// 同协议透传的收尾看护：补齐上游没给的 finish_reason / [DONE]
    tracker: PassthroughTracker,
    /// 上游已结束（EOF 之后不再 poll inner，避免依赖流的 fuse 行为）
    finished: bool,
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
        // 上游异常断开时补发的错误帧只发一次，发完还要继续走到 EOF 补收尾帧
        loop {
            if self.finished {
                return Poll::Ready(None);
            }
            match self.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    self.finished = true;
                    // 流正常结束：把 buffer 中残留的半行（最后一个 chunk 可能无换行）也处理掉
                    let rest = std::mem::take(&mut self.buffer);
                    let mut payload = None;
                    if !rest.is_empty() {
                        let text = String::from_utf8_lossy(&rest).into_owned();
                        payload = text.trim().strip_prefix("data:").map(|s| s.trim().to_string());
                        if let Some(p) = &payload {
                            let usage = extract_stream_usage(p);
                            if let Some((pi, po, cr, cw)) = usage {
                                self.stats.absorb_usage(pi, po, cr, cw);
                            }
                            self.tracker.observe(p, usage);
                        }
                    }
                    // 残留那一行本身就是终止哨兵（`[DONE]` / message_stop）时，
                    // 补的只能是它前面缺的终止帧——哨兵由这一行自己充当，不能再补一份。
                    let rest_is_sentinel = payload
                        .as_deref()
                        .map(|p| self.tracker.is_terminal_sentinel(p))
                        .unwrap_or(false);
                    let add = if rest_is_sentinel {
                        self.tracker.finish_only()
                    } else {
                        self.tracker.backfill()
                    };
                    if !add.is_empty() {
                        tracing::warn!(
                            "上游流未收尾即结束，已补发收尾帧（protocol={} tool_call={} sentinel={}）",
                            if self.tracker.anthropic { "anthropic" } else { "openai" },
                            self.tracker.saw_tool_call,
                            rest_is_sentinel
                        );
                        // 这种流实际是断掉的，统计上如实记为失败
                        self.stats.failed.store(true, Ordering::SeqCst);
                        if let Some(capture) = &mut self.capture {
                            capture.push(add.as_bytes());
                        }
                    }
                    // 顺序：终止帧必须在哨兵之前，客户端读到哨兵就收尾了
                    let mut out = Vec::with_capacity(rest.len() + add.len());
                    if rest_is_sentinel {
                        out.extend_from_slice(add.as_bytes());
                        out.extend_from_slice(&rest);
                    } else {
                        out.extend_from_slice(&rest);
                        out.extend_from_slice(add.as_bytes());
                    }
                    if let Some(capture) = &mut self.capture {
                        capture.flush();
                    }
                    self.stats.record_once();
                    if out.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    // 收到上游第一个数据 chunk：记录首字时长（CAS 幂等）
                    self.stats.mark_first_token();
                    if let Some(capture) = &mut self.capture {
                        capture.push(&bytes);
                    }
                    self.buffer.extend_from_slice(&bytes);
                    // 逐行解析已完整的 SSE 行：
                    // 完整行立即转发（不攒到 EOF，上游若在 [DONE] 之后保持连接也不会卡住客户端），
                    // 不完整的半行留在 buffer 里等下一个 chunk。
                    // 按行转发是必须的：只有落在事件边界上，才有机会把补发的终止帧
                    // 插在 `[DONE]` 之前（否则 `[DONE]` 被 TCP 切成两段时会漏判）。
                    let mut out = Vec::with_capacity(bytes.len());
                    while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = self.buffer.drain(..=pos).collect();
                        let text = String::from_utf8_lossy(&line).into_owned();
                        let payload = match text.trim().strip_prefix("data:") {
                            Some(rest) => rest.trim().to_string(),
                            None => {
                                out.extend_from_slice(&line);
                                continue;
                            }
                        };
                        let usage = extract_stream_usage(&payload);
                        if let Some((p, c, cr, cw)) = usage {
                            self.stats.absorb_usage(p, c, cr, cw);
                        }
                        // 补帧必须在哨兵之前：OpenAI SDK 读到 `[DONE]`、Anthropic SDK
                        // 读到 message_stop 都会立刻收尾，补在它们后面的终止帧永远读不到，
                        // 客户端照样报 Stream ended without finish_reason。
                        if self.tracker.is_terminal_sentinel(&payload) && !self.tracker.saw_finish_reason {
                            self.tracker.observe(&payload, usage);
                            let chunk = self.tracker.finish_only();
                            tracing::warn!(
                                "上游流给了终止哨兵却没给 {}，已在哨兵之前补发终止帧",
                                if self.tracker.anthropic { "stop_reason" } else { "finish_reason" }
                            );
                            self.stats.failed.store(true, Ordering::SeqCst);
                            out.extend_from_slice(chunk.as_bytes());
                            if let Some(capture) = &mut self.capture {
                                capture.push(chunk.as_bytes());
                            }
                            self.tracker.backfilled = true;
                            out.extend_from_slice(&line);
                            continue;
                        }
                        self.tracker.observe(&payload, usage);
                        out.extend_from_slice(&line);
                    }
                    if out.is_empty() {
                        // 整个 chunk 都是半行：不转发，继续拉上游
                        continue;
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Err(e))) => {
                    // 上游流错误：SSE 已经开始，改不了 HTTP 状态码，
                    // 直接返回错误会把事件流从中间截断（客户端只看到半个 chunk）。
                    // 正确做法是发一个 error 帧 + 补收尾帧，让客户端拿到明确原因。
                    tracing::warn!("同协议流式上游错误: {}", e);
                    self.stats.failed.store(true, Ordering::SeqCst);
                    let mut out = String::new();
                    if !self.tracker.backfilled {
                        if self.tracker.anthropic {
                            let evt = serde_json::json!({
                                "type": "error",
                                "error": {"type": "api_error", "message": "upstream stream error"}
                            });
                            out.push_str(&format!("event: error\ndata: {evt}\n\n"));
                        } else {
                            let evt = serde_json::json!({
                                "error": {"message": "upstream stream error", "type": "upstream_error"}
                            });
                            out.push_str(&format!("data: {evt}\n\n"));
                        }
                        self.tracker.backfilled = true;
                        out.push_str(&self.tracker.error_tail());
                    }
                    if let Some(capture) = &mut self.capture {
                        capture.push(out.as_bytes());
                        capture.flush();
                    }
                    self.stats.record_once();
                    self.finished = true;
                    if out.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
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
    // 客户端协议与上游相同，据此决定补发收尾帧的形态
    client_anthropic: bool,
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
        buffer: Vec::new(),
        stats,
        capture,
        tracker: PassthroughTracker::new(client_anthropic),
        finished: false,
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        // 反代（nginx）默认会缓冲 text/event-stream，这条头是让它别缓冲
        .insert_header(("X-Accel-Buffering", "no"))
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
        // 反代（nginx）默认会缓冲 text/event-stream，这条头是让它别缓冲
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(body)
}

/// 私有协议 adapter → SSE 归一化种类。标准协议返回 None。
fn private_protocol_kind(
    adapter: crate::models::adapter::PlatformAdapter,
) -> Option<crate::proxy::upstream::private_sse::ProtocolKind> {
    use crate::models::adapter::PlatformAdapter;
    use crate::proxy::upstream::private_sse::ProtocolKind;
    match adapter {
        PlatformAdapter::Trae => Some(ProtocolKind::Trae),
        PlatformAdapter::WorkBuddy => Some(ProtocolKind::WorkBuddy),
        _ => None,
    }
}

/// 把上游失败归类成「换号 / 冷却 / 直接失败」。
///
/// 私有协议先看响应体里的业务码（Trae 4008/4010/4015、WorkBuddy 14018 等），
/// 解析不出业务码时退回 HTTP 状态码；标准协议直接按状态码。
///
/// **这一步的正确性直接决定账号安全**：
/// - 把 4001（模型不在目录）当成可重试 → 每个账号都撞一遍，白等 N × 超时
/// - 把 4010/4015/4017（风控）当成配额去换号 → 单账号封禁升级成 IP 段封禁
fn classify_upstream_failure(
    adapter: crate::models::adapter::PlatformAdapter,
    status: u16,
    body: &str,
) -> crate::proxy::upstream::error_map::UpstreamFailure {
    use crate::models::adapter::PlatformAdapter;
    use crate::proxy::upstream::error_map as em;

    let business_code = match adapter {
        PlatformAdapter::Trae => crate::proxy::upstream::trae::parse_business_error(body)
            .and_then(|e| e.code),
        PlatformAdapter::WorkBuddy => crate::proxy::upstream::workbuddy::parse_business_error(body)
            .and_then(|e| e.code),
        _ => None,
    };

    match (adapter, business_code) {
        (PlatformAdapter::Trae, Some(code)) => em::classify_trae(code, body),
        (PlatformAdapter::WorkBuddy, Some(code)) => em::classify_workbuddy(Some(code), status, body),
        (PlatformAdapter::WorkBuddy, None) => em::classify_workbuddy(None, status, body),
        _ => em::classify_http(status, body),
    }
}

fn failure_message(failure: &crate::proxy::upstream::error_map::UpstreamFailure) -> &str {
    use crate::proxy::upstream::error_map::UpstreamFailure;
    match failure {
        UpstreamFailure::ClientError { message, .. }
        | UpstreamFailure::QuotaExhausted { message, .. }
        | UpstreamFailure::RiskControl { message, .. }
        | UpstreamFailure::Unauthorized { message }
        | UpstreamFailure::Transport { message }
        | UpstreamFailure::UpstreamBusy { message, .. } => message,
    }
}

/// 归一化种类 → adapter。流内业务错误需要知道是哪个协议才能正确分类。
fn adapter_of_kind(
    kind: crate::proxy::upstream::private_sse::ProtocolKind,
) -> crate::models::adapter::PlatformAdapter {
    use crate::models::adapter::PlatformAdapter;
    use crate::proxy::upstream::private_sse::ProtocolKind;
    match kind {
        ProtocolKind::Trae => PlatformAdapter::Trae,
        ProtocolKind::WorkBuddy => PlatformAdapter::WorkBuddy,
    }
}

/// 后端冷却表：backend_id → 冷却截止时刻。
///
/// 为什么放在 handler 而不是 `BackendSelector`：选择器只做「按策略挑一个」，
/// 冷却状态是「上游健康度」，属于代理层关注点。用进程内 DashMap 而不是 DB：
/// 冷却是一过性状态，重启后重新探测即可，没必要持久化。
fn cooldown_map() -> &'static dashmap::DashMap<String, std::time::Instant> {
    static COOLDOWN: once_cell::sync::Lazy<dashmap::DashMap<String, std::time::Instant>> =
        once_cell::sync::Lazy::new(dashmap::DashMap::new);
    &COOLDOWN
}

fn cooldown_backend(_state: &Arc<ProxyState>, backend_id: &str, secs: u64) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    cooldown_map().insert(backend_id.to_string(), until);
}

/// 该后端是否还在冷却中。冷却过期的条目顺手清掉，避免表无限增长。
fn is_cooling_down(backend_id: &str) -> bool {
    if let Some(entry) = cooldown_map().get(backend_id) {
        if *entry.value() > std::time::Instant::now() {
            return true;
        }
        drop(entry);
        cooldown_map().remove(backend_id);
    }
    false
}

/// 私有协议流式 body：把上游 SSE 归一化成 OpenAI 形态，
/// 再按客户端协议选择「直接透传 OpenAI 事件」或「转成 Anthropic 事件」。
struct PrivateStreamBody {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>,
    normalizer: crate::proxy::upstream::private_sse::PrivateSseNormalizer,
    /// 非 None 时把 OpenAI 形态再转成 Anthropic 事件
    converter: Option<crate::proxy::stream_convert::StreamConverter>,
    stats: Arc<StreamStatCtx>,
    capture: Option<StreamCapture>,
    finished: bool,
}

impl Drop for PrivateStreamBody {
    fn drop(&mut self) {
        self.stats.record_once();
        if let Some(capture) = &mut self.capture {
            capture.flush();
        }
    }
}

impl Stream for PrivateStreamBody {
    type Item = Result<Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::sync::atomic::Ordering;
        loop {
            if self.finished {
                return Poll::Ready(None);
            }
            match self.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    self.finished = true;
                    let events = self.normalizer.finish();
                    let mut out = String::new();
                    for event in events {
                        out.push_str(&self.render_event(event));
                    }
                    // Anthropic 客户端：归一化器只补了 OpenAI 形态的终止 chunk，
                    // message_delta / message_stop 得由转换器收尾才能发出来
                    out.push_str(&self.flush_converter());
                    if let Some(capture) = &mut self.capture {
                        capture.flush();
                    }
                    self.stats.record_once();
                    if out.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    self.stats.mark_first_token();
                    if let Some(capture) = &mut self.capture {
                        capture.push(&bytes);
                    }
                    let events = self.normalizer.push(&bytes);
                    let mut out = String::new();
                    for event in events {
                        out.push_str(&self.render_event(event));
                    }
                    if out.is_empty() {
                        // 这一帧没有可转发的内容（心跳/纯 usage），继续拉下一帧
                        continue;
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
                Poll::Ready(Some(Err(e))) => {
                    // 上游流错误：直接返回 Err 会把 SSE 从中间截断，客户端只看到
                    // 半条事件流（表现为 Stream ended without finish_reason）。
                    // 改成发错误帧 + 正常收尾，客户端能拿到明确原因。
                    tracing::warn!("私有协议上游流错误: {}", e);
                    self.stats.failed.store(true, Ordering::SeqCst);
                    self.finished = true;
                    let mut out = String::new();
                    if let Some(mut converter) = self.converter.take() {
                        // Anthropic 客户端：转换器负责 error 事件 + message_delta + message_stop
                        out.push_str(&converter.emit_error("upstream stream error"));
                        let (pi, po, cr, cw) = converter.usage();
                        self.stats.absorb_usage(pi, po, cr, cw);
                    } else {
                        let payload = serde_json::json!({
                            "error": {"message": "upstream stream error", "type": "upstream_error"}
                        });
                        out.push_str(&format!("data: {payload}\n\n"));
                        // 归一化器补一个带 finish_reason 的终止 chunk，最后是 [DONE]
                        for event in self.normalizer.finish() {
                            out.push_str(&self.render_event(event));
                        }
                        out.push_str("data: [DONE]\n\n");
                    }
                    if let Some(capture) = &mut self.capture {
                        capture.push(out.as_bytes());
                        capture.flush();
                    }
                    self.stats.record_once();
                    if out.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(out))));
                }
            }
        }
    }
}

impl PrivateStreamBody {
    /// 收尾转换器（仅 Anthropic 客户端会走这条）。
    /// 归一化器发出的终止 chunk 会让转换器进入 finished，此处 take 掉并补发剩余事件。
    fn flush_converter(&mut self) -> String {
        match self.converter.take() {
            Some(mut converter) => {
                let out = converter.finish();
                let (pi, po, cr, cw) = converter.usage();
                self.stats.absorb_usage(pi, po, cr, cw);
                out
            }
            None => String::new(),
        }
    }

    /// 把一个归一化事件渲染成要发给客户端的字节。
    fn render_event(
        &mut self,
        event: crate::proxy::upstream::private_sse::NormalizedEvent,
    ) -> String {
        use crate::proxy::upstream::private_sse::NormalizedEvent;
        match event {
            NormalizedEvent::Ignore => String::new(),
            NormalizedEvent::BusinessError(err) => {
                tracing::warn!("私有协议流内业务错误: code={:?} msg={}", err.code, err.message);
                self.stats.failed.store(true, std::sync::atomic::Ordering::SeqCst);
                // 流已开始，无法改 HTTP 状态码；发一个 error 帧让客户端能看到原因
                let payload = serde_json::json!({
                    "error": {
                        "message": err.message,
                        "type": "upstream_error",
                        "code": err.code
                    }
                });
                format!("data: {payload}\n\n")
            }
            NormalizedEvent::Chunk(chunk) => {
                // 统计旁路：从归一化后的 chunk 里取 usage
                if let Some(usage) = chunk.get("usage") {
                    let p = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let c = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let cr = usage
                        .get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if p > 0 || c > 0 || cr > 0 {
                        self.stats.absorb_usage(p, c, cr, 0);
                    }
                }
                let payload = chunk.to_string();
                if let Some(converter) = self.converter.as_mut() {
                    // 客户端是 Anthropic：把 OpenAI chunk 转成 Anthropic 事件
                    let line = format!("data: {payload}\n\n");
                    converter.push_chunk(line.as_bytes())
                } else {
                    format!("data: {payload}\n\n")
                }
            }
        }
    }
}

/// 处理私有协议流式响应。
#[allow(clippy::too_many_arguments)]
fn handle_private_stream(
    resp: reqwest::Response,
    stats_writer: &StatsWriter,
    stat_base: RequestStat,
    pricing: Option<crate::models::pricing::Pricing>,
    kind: crate::proxy::upstream::private_sse::ProtocolKind,
    client_anthropic: bool,
    model: &str,
    backend_id: &str,
    upstream_model: &str,
    start: Instant,
    pricing_at: chrono::DateTime<chrono::Local>,
    capture: Option<StreamCapture>,
) -> HttpResponse {
    let stats = Arc::new(StreamStatCtx::new(stat_base, stats_writer, start, pricing, pricing_at));
    let converter = if client_anthropic {
        Some(crate::proxy::stream_convert::StreamConverter::new(
            true,
            model,
            backend_id,
            upstream_model,
        ))
    } else {
        None
    };
    let body = PrivateStreamBody {
        inner: Box::pin(resp.bytes_stream()),
        normalizer: crate::proxy::upstream::private_sse::PrivateSseNormalizer::new(kind, model),
        converter,
        stats,
        capture,
        finished: false,
    };
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        // 反代（nginx）默认会缓冲 text/event-stream，这条头是让它别缓冲
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(body)
}

/// 私有协议的非流式聚合：把整条 SSE 读完，拼成一个 `chat.completion` 响应。
///
/// 为什么必须这么做：Trae 与 WorkBuddy 的上游**无论客户端要什么都回 SSE**，
/// 直接把响应体当 JSON 解析会得到空对象（表现为「模型没返回内容」）。
#[allow(clippy::too_many_arguments)]
async fn handle_private_aggregate(
    resp: reqwest::Response,
    stats_writer: &StatsWriter,
    stat_base: RequestStat,
    pricing: Option<crate::models::pricing::Pricing>,
    kind: crate::proxy::upstream::private_sse::ProtocolKind,
    client_anthropic: bool,
    model: &str,
    start: Instant,
    pricing_at: chrono::DateTime<chrono::Local>,
) -> HttpResponse {
    use futures_util::StreamExt;

    let mut normalizer = crate::proxy::upstream::private_sse::PrivateSseNormalizer::new(kind, model);
    let mut stream = resp.bytes_stream();
    let mut failed = false;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                for event in normalizer.push(&bytes) {
                    if let crate::proxy::upstream::private_sse::NormalizedEvent::BusinessError(err) = event {
                        tracing::warn!("私有协议聚合遇到业务错误: code={:?} msg={}", err.code, err.message);
                        failed = true;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("私有协议聚合时上游流错误: {}", e);
                failed = true;
                break;
            }
        }
    }
    let _ = normalizer.finish();
    let latency_ms = start.elapsed().as_millis() as i64;

    // 统计：token 从 usage 里取（上游不一定给，缺就记 None）
    let (token_in, token_out) = normalizer
        .usage
        .as_ref()
        .map(|u| {
            (
                u.get("prompt_tokens").and_then(|v| v.as_i64()),
                u.get("completion_tokens").and_then(|v| v.as_i64()),
            )
        })
        .unwrap_or((None, None));

    let mut stat = stat_base;
    stat.latency_ms = latency_ms;
    stat.token_input = token_in;
    stat.token_output = token_out;
    if failed {
        stat.status_code = 0;
        stat.error_type = Some("StreamError".to_string());
    }
    let _ = pricing;
    let _ = pricing_at;
    stats_writer.record(stat);

    // 流内业务错误必须变成真正的错误响应。
    //
    // 这是上游协议的一个陷阱：Trae / WorkBuddy 把配额耗尽、风控这类**业务失败**
    // 放在 `200 OK` 的 SSE 里（Trae 4008、WorkBuddy 14018）。如果只当成一个普通
    // 事件吞掉，客户端会收到「成功但 content 为空」的响应，完全看不出是额度问题。
    if let Some(err) = normalizer.business_error.clone() {
        // 流内错误没有 HTTP 状态码，必须走专用入口（否则风控码会被误判成请求级错误）
        let failure = crate::proxy::upstream::error_map::classify_business_error(
            adapter_of_kind(kind),
            err.code,
            &err.message,
        );
        let client_status = failure.client_status();
        tracing::warn!(
            "私有协议流内业务错误：code={:?} → 对客户端回 {}（{:?}）",
            err.code,
            client_status,
            failure
        );
        let message = if err.message.is_empty() {
            format!("upstream business error {:?}", err.code)
        } else {
            err.message.clone()
        };
        return HttpResponse::build(
            actix_web::http::StatusCode::from_u16(client_status)
                .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
        )
        .json(serde_json::json!({
            "error": {
                "message": message,
                "type": "upstream_error",
                "code": err.code,
            }
        }));
    }

    let response = normalizer.into_response();
    if client_anthropic {
        // 客户端要 Anthropic 形态：先按 OpenAI 解析成统一格式，再用现有转换器输出。
        // 解析失败时不能静默回 OpenAI 形态——那会让 Anthropic 客户端收到看不懂的响应，
        // 必须记日志（这种失败通常意味着归一化产出的 chunk 形态有问题）。
        match crate::protocol::openai::parse_response(response.clone()) {
            Ok(unified) => {
                return HttpResponse::Ok().json(crate::protocol::anthropic::to_response(&unified));
            }
            Err(e) => {
                tracing::error!("私有协议聚合后按 OpenAI 解析失败（将回退 OpenAI 形态）: {}", e);
            }
        }
    }
    HttpResponse::Ok().json(response)
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
    use super::{sanitize_forward_url, truncate_utf8, PassthroughTracker};
    use actix_web::web::Bytes;

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

    // ============ 同协议透传的收尾看护 ============

    /// 喂若干 payload（模拟 `data:` 后面的内容），返回 (tracker, 收尾帧)
    fn feed(anthropic: bool, payloads: &[&str]) -> (PassthroughTracker, String) {
        let mut t = PassthroughTracker::new(anthropic);
        for p in payloads {
            let usage = super::extract_stream_usage(p);
            t.observe(p, usage);
        }
        let tail = t.backfill();
        (t, tail)
    }

    #[test]
    fn complete_openai_stream_is_passed_through_untouched() {
        // 上游给了 finish_reason 和 [DONE]：一个字节都不该补，否则是重复帧
        let (_, tail) = feed(
            false,
            &[
                r#"{"id":"a","model":"m","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#,
                r#"{"id":"a","model":"m","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
                "[DONE]",
            ],
        );
        assert!(tail.is_empty(), "正常收尾的流不该被补帧，实际补了：{tail}");
    }

    #[test]
    fn truncated_openai_stream_gets_finish_reason_and_done() {
        // 复现实测的那 0.3%：上游生成中途断流，既没 finish_reason 也没 [DONE]
        let (t, tail) = feed(
            false,
            &[
                r#"{"id":"chatcmpl-1","model":"agnes","created":1700000000,"choices":[{"index":0,"delta":{"content":"半句"},"finish_reason":null}]}"#,
                r#"{"id":"chatcmpl-1","model":"agnes","created":1700000000,"choices":[{"index":0,"delta":{"content":"话"},"finish_reason":null}]}"#,
            ],
        );
        assert!(t.needs_backfill());
        let frames: Vec<&str> = tail.split("\n\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(frames.len(), 2, "应补 finish 帧 + [DONE]，实际：{tail}");
        let chunk: serde_json::Value =
            serde_json::from_str(frames[0].strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(chunk["choices"][0]["finish_reason"], "stop");
        // 沿用上游的 id/model/created，客户端才认得出这是同一条响应
        assert_eq!(chunk["id"], "chatcmpl-1");
        assert_eq!(chunk["model"], "agnes");
        assert_eq!(chunk["created"], 1700000000i64);
        assert_eq!(frames[1], "data: [DONE]");
    }

    #[test]
    fn done_without_finish_reason_still_gets_finish_reason() {
        // 只有 [DONE] 没有 finish_reason：客户端仍会报 Stream ended without finish_reason
        let (_, tail) = feed(
            false,
            &[
                r#"{"choices":[{"index":0,"delta":{"content":"x"},"finish_reason":null}]}"#,
                "[DONE]",
            ],
        );
        assert!(tail.contains(r#""finish_reason":"stop""#), "实际：{tail}");
        // [DONE] 已经给过了，不能再补一个
        assert_eq!(tail.matches("[DONE]").count(), 0, "不该重复补 [DONE]：{tail}");
    }

    #[test]
    fn finish_reason_without_done_only_gets_done() {
        let (_, tail) = feed(
            false,
            &[
                r#"{"choices":[{"index":0,"delta":{"content":"x"},"finish_reason":"length"}]}"#,
            ],
        );
        assert_eq!(tail, "data: [DONE]\n\n");
    }

    #[test]
    fn truncated_tool_call_stream_reports_tool_calls() {
        // 断在 tool_calls 中间：补 stop 会让客户端丢掉工具调用，必须补 tool_calls
        let (_, tail) = feed(
            false,
            &[r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"f","arguments":""}}]},"finish_reason":null}]}"#],
        );
        assert!(tail.contains(r#""finish_reason":"tool_calls""#), "实际：{tail}");
    }

    #[test]
    fn complete_anthropic_stream_is_passed_through_untouched() {
        let (_, tail) = feed(
            true,
            &[
                r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
                r#"{"type":"message_stop"}"#,
            ],
        );
        assert!(tail.is_empty(), "正常收尾的流不该被补帧，实际补了：{tail}");
    }

    #[test]
    fn truncated_anthropic_stream_gets_delta_then_stop() {
        // OpenAI SDK 的 Stream ended without finish_reason 在 Anthropic 侧对应
        // stopReason 停在 "pending"，靠 message_delta 的 stop_reason 解掉
        let (_, tail) = feed(
            true,
            &[
                r#"{"type":"message_start","message":{"usage":{"input_tokens":5}}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"半句"}}"#,
            ],
        );
        let delta_at = tail.find("event: message_delta").expect("缺 message_delta");
        let stop_at = tail.find("event: message_stop").expect("缺 message_stop");
        assert!(delta_at < stop_at, "message_delta 必须在 message_stop 之前：{tail}");
        let data: &str = tail
            .lines()
            .find(|l| l.starts_with("data: {") && l.contains("message_delta"))
            .unwrap()
            .trim_start_matches("data: ");
        let evt: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(evt["delta"]["stop_reason"], "end_turn");
        assert_eq!(evt["usage"]["output_tokens"], 0);
    }

    #[test]
    fn anthropic_tool_use_block_marks_tool_call() {
        // Anthropic 客户端这条上不写 stop_reason 的值，但 tool_use 出现必须被记住，
        // 以免将来改成按工具改写时静默丢工具
        let (t, _) = feed(
            true,
            &[r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tu1","name":"f"}}"#],
        );
        assert!(t.saw_tool_call);
    }

    #[test]
    fn backfill_happens_only_once() {
        let mut t = PassthroughTracker::new(false);
        t.observe(r#"{"choices":[{"index":0,"delta":{"content":"x"},"finish_reason":null}]}"#, None);
        let first = t.backfill();
        assert!(!first.is_empty());
        assert!(t.backfill().is_empty(), "第二次不该再补帧");
    }

    #[test]
    fn error_tail_is_always_complete_regardless_of_backfilled_flag() {
        // 上游报错那条路：错误帧之后必须还有完整的终止序列，否则
        // 客户端只会看到半条流并报 Stream ended without finish_reason
        let mut t = PassthroughTracker::new(false);
        t.observe(r#"{"id":"a","choices":[{"index":0,"delta":{"content":"x"},"finish_reason":null}]}"#, None);
        let _ = t.backfill();
        t.backfilled = true;
        let tail = t.error_tail();
        assert!(tail.contains(r#""finish_reason":"stop""#), "实际：{tail}");
        assert!(tail.contains("data: [DONE]"));
    }

    #[test]
    fn malformed_payload_is_ignored_not_panicked() {
        let (t, tail) = feed(false, &["not json", "{broken", ""]);
        // 没看到任何收尾信息 → 仍然按需要补帧，且不 panic
        assert!(t.needs_backfill());
        assert!(tail.contains("data: [DONE]"));
    }

    // ============ 端到端：真正跑 StreamPassthroughBody 的 poll 循环 ============

    /// 临时库上的 StatsWriter（只为让 body 能构造出来；不校验落库结果）
    fn test_stats_writer(tag: &str) -> (crate::cache::StatsWriter, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("stream-{tag}-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::init_pool(&path).expect("init test db");
        (
            crate::cache::StatsWriter::start(pool, crate::events::EventHub::new()),
            path,
        )
    }

    fn test_stat_base() -> crate::models::stats::RequestStat {
        crate::models::stats::RequestStat {
            id: 0,
            proxy_id: "p".into(),
            route_id: "r".into(),
            backend_id: "b".into(),
            status_code: 200,
            latency_ms: 0,
            token_input: None,
            token_output: None,
            error_type: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            ttft_ms: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            is_stream: true,
            api_key_id: None,
            cost: None,
            cache_savings: None,
            request_id: None,
            replayed_from: None,
            path: None,
            credits: None,
        }
    }

    /// 用一批假的上游 chunk 跑完整的 body，收集实际发给客户端的字节
    async fn run_body(
        tag: &str,
        anthropic: bool,
        chunks: Vec<Result<Bytes, reqwest::Error>>,
    ) -> String {
        use futures_util::StreamExt;
        let (writer, path) = test_stats_writer(tag);
        let stats = std::sync::Arc::new(super::StreamStatCtx::new(
            test_stat_base(),
            &writer,
            std::time::Instant::now(),
            None,
            chrono::Local::now(),
        ));
        let body = super::StreamPassthroughBody {
            inner: Box::pin(futures_util::stream::iter(chunks)),
            buffer: Vec::new(),
            stats,
            capture: None,
            tracker: PassthroughTracker::new(anthropic),
            finished: false,
        };
        let collected: Vec<Result<Bytes, actix_web::Error>> = Box::pin(body).collect().await;
        let mut out = String::new();
        for item in collected {
            out.push_str(&String::from_utf8_lossy(&item.expect("body 不该返回 Err")));
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
        out
    }

    #[actix_web::test]
    async fn wall_wire_truncated_stream_gets_terminal_frames() {
        // 上游真实形态：内容帧若干，然后连接直接断掉（没有 finish_reason、没有 [DONE]）
        let out = run_body(
            "trunc",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"id\":\"chatcmpl-9\",\"model\":\"agnes\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"半\"},\"finish_reason\":null}]}\n\n",
                )),
                Ok(Bytes::from(
                    "data: {\"id\":\"chatcmpl-9\",\"model\":\"agnes\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"句\"},\"finish_reason\":null}]}\n\n",
                )),
            ],
        )
        .await;
        // 上游两帧原样还在
        assert_eq!(out.matches("\"content\":\"半\"").count(), 1);
        assert_eq!(out.matches("\"content\":\"句\"").count(), 1);
        // 末尾补上了终止序列，客户端不会再报 Stream ended without finish_reason
        assert!(out.contains(r#""finish_reason":"stop""#), "实际：{out}");
        assert!(out.trim_end().ends_with("data: [DONE]"), "实际：{out}");
        assert_eq!(out.matches("data: [DONE]").count(), 1);
    }

    #[actix_web::test]
    async fn wall_wire_done_without_finish_reason_gets_frame_before_the_sentinel() {
        // 上游给了 [DONE] 却没给 finish_reason。
        // 终止帧必须落在 [DONE] 之前：OpenAI SDK 读到 [DONE] 立刻 break，
        // 补在它后面的帧永远读不到，客户端照样报 Stream ended without finish_reason。
        let out = run_body(
            "doneonly",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"半句\"},\"finish_reason\":null}]}\n\n",
                )),
                Ok(Bytes::from("data: [DONE]\n\n")),
            ],
        )
        .await;
        let finish_at = out.find(r#""finish_reason":"stop""#).expect("必须补终止帧");
        let done_at = out.find("data: [DONE]").expect("[DONE] 必须在");
        assert!(finish_at < done_at, "终止帧必须在 [DONE] 之前，实际：{out}");
        assert_eq!(out.matches("data: [DONE]").count(), 1, "哨兵不能被补重复：{out}");
        assert_eq!(out.matches(r#""finish_reason":"stop""#).count(), 1, "实际：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_done_split_across_chunks_still_gets_terminal_frame() {
        // `[DONE]` 被 TCP 切成两段：按行转发才能落在事件边界上判断
        let out = run_body(
            "split",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"},\"finish_reason\":null}]}\n\ndata: [DO",
                )),
                Ok(Bytes::from("NE]\n\n")),
            ],
        )
        .await;
        let finish_at = out.find(r#""finish_reason":"stop""#).expect("必须补终止帧");
        let done_at = out.find("data: [DONE]").expect("[DONE] 必须在");
        assert!(finish_at < done_at, "实际：{out}");
        assert_eq!(out.matches("data: [DONE]").count(), 1, "实际：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_multibyte_char_split_across_chunks_is_not_mangled() {
        // 一个 chunk 可能把一个汉字切成两半。缓冲必须是字节级的：
        // 用 String::from_utf8_lossy 逐 chunk 拼接会把汉字写成 U+FFFD。
        let full = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"半句话\"},\"finish_reason\":null}]}\n\n";
        let bytes = full.as_bytes();
        // 切在第二个字符「句」的第一个字节之后（3 字节 UTF-8）
        let cut = full.find('句').expect("测试数据里应有「句」") + 1;
        assert!(cut < bytes.len());
        let out = run_body(
            "mb",
            false,
            vec![
                Ok(Bytes::from(bytes[..cut].to_vec())),
                Ok(Bytes::from(bytes[cut..].to_vec())),
            ],
        )
        .await;
        assert!(!out.contains('\u{FFFD}'), "汉字被写坏了：{out}");
        assert!(out.contains("半句话"), "内容应完整还原：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_anthropic_message_stop_without_stop_reason_gets_delta_before_it() {
        // Anthropic 上游给了 message_stop 却没给 stop_reason。
        // message_delta 必须落在 message_stop 之前，否则 stopReason 停在 pending，
        // 客户端报 Stream ended without finish_reason。
        let out = run_body(
            "anth_done",
            true,
            vec![
                Ok(Bytes::from(
                    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n\n",
                )),
                Ok(Bytes::from(
                    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"半句\"}}\n\n",
                )),
                Ok(Bytes::from("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n")),
            ],
        )
        .await;
        let delta_at = out.find(r#""stop_reason":"end_turn""#).expect("必须补 message_delta");
        let stop_at = out.find("\"type\":\"message_stop\"").expect("message_stop 必须在");
        assert!(delta_at < stop_at, "message_delta 必须在 message_stop 之前，实际：{out}");
        // 上游的 message_stop 原样保留且只有一份
        assert_eq!(out.matches("\"type\":\"message_stop\"").count(), 1, "实际：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_sentinel_without_trailing_newline_still_gets_frame_before_it() {
        // 上游最后一帧 `[DONE]` 没有换行就断流：它会留在 buffer 残行里，
        // 补的终止帧仍必须排在它之前。
        let out = run_body(
            "nonl",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"},\"finish_reason\":null}]}\n\n",
                )),
                Ok(Bytes::from("data: [DONE]")),
            ],
        )
        .await;
        let finish_at = out.find(r#""finish_reason":"stop""#).expect("必须补终止帧");
        let done_at = out.find("data: [DONE]").expect("[DONE] 必须在");
        assert!(finish_at < done_at, "实际：{out}");
        assert_eq!(out.matches("data: [DONE]").count(), 1, "哨兵不能重复：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_complete_stream_is_byte_identical() {
        // 正常流一个字节都不能变：透传是热路径，补帧只能发生在异常流上
        let expected = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let out = run_body(
            "intact",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
                )),
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                )),
            ],
        )
        .await;
        assert_eq!(out, expected);
    }

    #[actix_web::test]
    async fn wall_wire_upstream_error_does_not_truncate_the_event_stream() {
        // 上游在流中途报错：以前直接返回 Err 会把 SSE 截断，
        // 客户端只看到半条流 → 报 Stream ended without finish_reason
        let err = reqwest::Client::new()
            .get("http://[")
            .build()
            .expect_err("非法 URL 应产生 reqwest::Error");
        let out = run_body(
            "err",
            false,
            vec![
                Ok(Bytes::from(
                    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"},\"finish_reason\":null}]}\n\n",
                )),
                Err(err),
            ],
        )
        .await;
        // 错误原因对客户端可见
        assert!(out.contains("\"error\""), "实际：{out}");
        assert!(out.contains("upstream stream error"), "实际：{out}");
        // 而且收尾序列完整
        assert!(out.contains(r#""finish_reason":"stop""#), "实际：{out}");
        assert!(out.trim_end().ends_with("data: [DONE]"), "实际：{out}");
    }

    #[actix_web::test]
    async fn wall_wire_truncated_anthropic_stream_ends_with_message_stop() {
        let out = run_body(
            "anth",
            true,
            vec![Ok(Bytes::from(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"半句\"}}\n\n",
            ))],
        )
        .await;
        assert!(out.trim_end().ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}"), "实际：{out}");
        // stop_reason 必须出现，否则 Anthropic 客户端 stopReason 停在 pending
        assert!(out.contains(r#""stop_reason":"end_turn""#), "实际：{out}");
    }
}
