use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::db::DbPool;
use crate::error::AppError;
use crate::models::api_key::StoredApiKey;
use crate::models::platform::Platform;
use crate::models::route::{Backend, LBStrategy, RetryPolicy};
use crate::models::stats::{RequestAttempt, RequestStat};

/// 已解析的后端：包含完整的 platform 信息，请求时零 DB 查询
#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub backend: Backend,
    pub platform: Platform,
}

/// 已解析的路由：包含所有后端及其对应平台信息
#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub route_id: String,
    pub proxy_id: String,
    pub backends: Vec<ResolvedBackend>,
    pub lb_strategy: LBStrategy,
    pub retry_policy: RetryPolicy,
    pub fallback: Option<String>,
    /// 客户端未传 max_tokens 时填充的默认值（None = 不干预）
    pub default_max_tokens: Option<u32>,
}

/// 缓存条目：proxy name → ResolvedRoute
#[derive(Debug, Clone)]
struct CacheEntry {
    route: ResolvedRoute,
}

/// 请求热路径的内存缓存，消除所有同步 DB 查询
#[derive(Clone)]
pub struct RouteCache {
    inner: Arc<RouteCacheInner>,
}

struct RouteCacheInner {
    /// proxy name → ResolvedRoute
    entries: DashMap<String, CacheEntry>,
    /// API key 缓存：sha256(密钥明文) → 行（零 DB 查询验证，库里也没有明文可对）
    api_keys: DashMap<String, StoredApiKey>,
    /// DB 连接池（仅用于刷新缓存）
    db: DbPool,
}

impl RouteCache {
    /// 创建缓存并从 DB 加载初始数据
    pub fn new(db: DbPool) -> Self {
        let cache = Self {
            inner: Arc::new(RouteCacheInner {
                entries: DashMap::new(),
                api_keys: DashMap::new(),
                db,
            }),
        };
        // 初始加载
        cache.refresh();
        cache.refresh_api_keys();
        cache
    }

    /// 从 DB 重建整个缓存（在管理操作后调用）
    pub fn refresh(&self) {
        let db = &self.inner.db;
        let proxies = match crate::db::proxy::list(db) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("刷新缓存失败（获取代理列表）: {}", e);
                return;
            }
        };

        let mut new_entries = Vec::new();

        for proxy in &proxies {
            let route = match crate::db::route::get_by_proxy(db, &proxy.id) {
                Ok(Some(r)) => r,
                _ => continue,
            };

            let mut resolved_backends = Vec::new();
            for backend in &route.backends {
                match crate::db::platform::get(db, &backend.platform_id) {
                    Ok(platform) => {
                        resolved_backends.push(ResolvedBackend {
                            backend: backend.clone(),
                            platform,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("跳过后端 {}（平台 {} 查询失败: {}）", backend.id, backend.platform_id, e);
                    }
                }
            }

            new_entries.push((
                proxy.name.clone(),
                CacheEntry {
                    route: ResolvedRoute {
                        route_id: route.id,
                        proxy_id: route.proxy_id,
                        backends: resolved_backends,
                        lb_strategy: route.lb_strategy,
                        retry_policy: route.retry_policy,
                        fallback: route.fallback,
                        default_max_tokens: route.default_max_tokens,
                    },
                },
            ));
        }

        // 全量替换
        self.inner.entries.clear();
        for (name, entry) in new_entries {
            self.inner.entries.insert(name, entry);
        }

        tracing::info!("路由缓存已刷新，共 {} 个代理", self.inner.entries.len());
    }

    /// 按代理名查找已解析的路由（零 DB 查询）
    pub fn get_resolved_route(&self, proxy_name: &str) -> Option<ResolvedRoute> {
        self.inner.entries.get(proxy_name).map(|r| r.route.clone())
    }

    /// 刷新 API key 缓存（在 API key 增删改后调用）
    pub fn refresh_api_keys(&self) {
        match crate::db::api_key::list(&self.inner.db) {
            Ok(keys) => {
                self.inner.api_keys.clear();
                for key in keys {
                    self.inner.api_keys.insert(key.key_hash.clone(), key);
                }
                tracing::info!("API key 缓存已刷新，共 {} 个", self.inner.api_keys.len());
            }
            Err(e) => {
                tracing::error!("刷新 API key 缓存失败: {}", e);
            }
        }
    }

    /// 是否配置了任何 API key（未配置时兼容旧行为：允许访问）
    pub fn has_api_keys(&self) -> bool {
        !self.inner.api_keys.is_empty()
    }

    /// 返回可用的 API key：启用且未到期。禁用、过期或不存在时返回 None（纯内存查询）。
    /// 入参仍是客户端送来的明文——它只用来算摘要，不会被存下或写出。
    /// 过期判定读的是缓存里的时间戳与当前时钟，所以到点即失效，不需要等一次缓存刷新。
    pub fn find_enabled_api_key(&self, key: &str) -> Option<StoredApiKey> {
        self.find_api_key(key)
            .filter(|api_key| api_key.enabled && !api_key.is_expired(chrono::Utc::now()))
    }

    /// 按明文找到那把 Key（不管启停与到期），用于把"不存在/已禁用"和"已过期"分开报。
    pub fn find_api_key(&self, key: &str) -> Option<StoredApiKey> {
        self.inner.api_keys.get(&crate::db::api_key::hash_key(key))
            .map(|api_key| api_key.clone())
    }

    /// 检查 API key 是否有效（纯内存查询，零 DB 交互）
    pub fn validate_api_key(&self, key: &str) -> Result<bool, AppError> {
        // 如果没有任何 API key 配置，允许所有访问
        if !self.has_api_keys() {
            return Ok(true);
        }
        Ok(self.find_enabled_api_key(key).is_some())
    }

    /// 记录 API key 最后使用时间（异步发送，不阻塞请求）
    /// 调用方手上已经有这一行了，所以这里只要 id，不再查一次表也不重算摘要。
    pub fn touch_api_key(&self, api_key_id: &str) {
        let db = self.inner.db.clone();
        let id = api_key_id.to_string();
        tokio::spawn(async move {
            let _ = crate::db::api_key::update_last_used(&db, &id);
        });
    }
}

/// 后台统计写入器：通过 channel 接收统计数据，批量异步写入 DB。
/// 请求统计与重试轨迹共用同一个后台任务（同一条热路径、同样只写本地 SQLite），
/// 两条通道都非阻塞：队列再长也不反压请求，宁可丢轨迹。
#[derive(Clone)]
pub struct StatsWriter {
    tx: mpsc::UnboundedSender<RequestStat>,
    attempt_tx: mpsc::UnboundedSender<RequestAttempt>,
    body_tx: mpsc::UnboundedSender<crate::models::stats::RequestBody>,
    headers_tx: mpsc::UnboundedSender<crate::models::stats::RequestHeaders>,
    response_tx: mpsc::UnboundedSender<crate::models::stats::ResponseBody>,
}

impl StatsWriter {
    /// 启动后台写入任务，返回 StatsWriter 发送端。
    /// `hub` 只被后台任务用来在落库后通知控制台：统计是攒一批 500ms 才写的，
    /// 不广播就没人知道该刷新了；发送端本身不需要事件总线。
    pub fn start(db: DbPool, hub: crate::events::EventHub) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<RequestStat>();
        let (attempt_tx, mut attempt_rx) = mpsc::unbounded_channel::<RequestAttempt>();
        let (body_tx, mut body_rx) = mpsc::unbounded_channel::<crate::models::stats::RequestBody>();
        let (headers_tx, mut headers_rx) = mpsc::unbounded_channel::<crate::models::stats::RequestHeaders>();
        let (response_tx, mut response_rx) = mpsc::unbounded_channel::<crate::models::stats::ResponseBody>();
        let events = hub;

        tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(64);
            let mut attempt_buffer = Vec::with_capacity(64);
            let mut body_buffer = Vec::with_capacity(64);
            let mut headers_buffer = Vec::with_capacity(64);
            let mut response_buffer = Vec::with_capacity(64);
            loop {
                // 收集一批数据，最多等 500ms
                let timeout = tokio::time::sleep(std::time::Duration::from_millis(500));
                tokio::pin!(timeout);

                loop {
                    tokio::select! {
                        stat = rx.recv() => {
                            match stat {
                                Some(s) => buffer.push(s),
                                None => {
                                    Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                                    return;
                                }
                            }
                        }
                        attempt = attempt_rx.recv() => {
                            match attempt {
                                Some(a) => attempt_buffer.push(a),
                                None => {
                                    Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                                    return;
                                }
                            }
                        }
                        body = body_rx.recv() => {
                            match body {
                                Some(b) => body_buffer.push(b),
                                None => {
                                    Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                                    return;
                                }
                            }
                        }
                        headers = headers_rx.recv() => {
                            match headers {
                                Some(h) => headers_buffer.push(h),
                                None => {
                                    Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                                    return;
                                }
                            }
                        }
                        response = response_rx.recv() => {
                            match response {
                                Some(r) => response_buffer.push(r),
                                None => {
                                    Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                                    return;
                                }
                            }
                        }
                        _ = &mut timeout => {
                            break;
                        }
                    }
                }

                let mut written = 0usize;
                let mut failed = 0usize;
                if !buffer.is_empty() {
                    failed = buffer
                        .iter()
                        .filter(|s| s.status_code >= 400 || s.error_type.is_some())
                        .count();
                    written = buffer.len();
                }
                Self::flush_all(&db, &mut buffer, &mut attempt_buffer, &mut body_buffer, &mut headers_buffer, &mut response_buffer);
                if written > 0 {
                    events.publish(crate::events::ConsoleEvent::Requests { count: written, failures: failed });
                }
            }
        });

        Self { tx, attempt_tx, body_tx, headers_tx, response_tx }
    }

    fn flush_all(
        db: &DbPool,
        buffer: &mut Vec<RequestStat>,
        attempt_buffer: &mut Vec<RequestAttempt>,
        body_buffer: &mut Vec<crate::models::stats::RequestBody>,
        headers_buffer: &mut Vec<crate::models::stats::RequestHeaders>,
        response_buffer: &mut Vec<crate::models::stats::ResponseBody>,
    ) {
        if !buffer.is_empty() {
            Self::flush(db, buffer);
        }
        if !attempt_buffer.is_empty() {
            Self::flush_attempts(db, attempt_buffer);
        }
        if !body_buffer.is_empty() {
            Self::flush_bodies(db, body_buffer);
        }
        if !headers_buffer.is_empty() {
            Self::flush_headers(db, headers_buffer);
        }
        if !response_buffer.is_empty() {
            Self::flush_responses(db, response_buffer);
        }
    }

    fn flush(db: &DbPool, buffer: &mut Vec<RequestStat>) {
        for stat in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record(db, &stat) {
                tracing::warn!("写入统计数据失败: {}", e);
            }
        }
    }

    fn flush_attempts(db: &DbPool, buffer: &mut Vec<RequestAttempt>) {
        for attempt in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record_attempt(db, &attempt) {
                tracing::warn!("写入重试轨迹失败: {}", e);
            }
        }
    }

    /// 非阻塞发送统计数据
    pub fn record(&self, stat: RequestStat) {
        // channel 满了就丢弃，不阻塞请求
        let _ = self.tx.send(stat);
    }

    /// 非阻塞发送一次尝试的轨迹；写失败只影响"能不能回溯重试过程"，不影响请求本身
    pub fn record_attempt(&self, attempt: RequestAttempt) {
        let _ = self.attempt_tx.send(attempt);
    }

    fn flush_bodies(db: &DbPool, buffer: &mut Vec<crate::models::stats::RequestBody>) {
        for body in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record_body(db, &body) {
                tracing::warn!("写入请求正文失败: {}", e);
            }
        }
    }

    fn flush_headers(db: &DbPool, buffer: &mut Vec<crate::models::stats::RequestHeaders>) {
        for headers in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record_headers(db, &headers) {
                tracing::warn!("写入请求头失败: {}", e);
            }
        }
    }

    fn flush_responses(db: &DbPool, buffer: &mut Vec<crate::models::stats::ResponseBody>) {
        for response in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record_response_body(db, &response) {
                tracing::warn!("写入响应正文失败: {}", e);
            }
        }
    }

    /// 非阻塞发送请求正文。只有采集开着时才会被调用（见 proxy::handler::capture_body）；
    /// 与统计一样绝不阻塞请求，也绝不在请求线程上写 SQL。
    pub fn record_body(&self, body: crate::models::stats::RequestBody) {
        let _ = self.body_tx.send(body);
    }

    /// 非阻塞发送请求头；与正文独立，长请求即使正文超限也能留下头。
    pub fn record_headers(&self, headers: crate::models::stats::RequestHeaders) {
        let _ = self.headers_tx.send(headers);
    }

    /// 非阻塞发送上游原始响应正文（已截断）。
    pub fn record_response_body(&self, response: crate::models::stats::ResponseBody) {
        let _ = self.response_tx.send(response);
    }
}
