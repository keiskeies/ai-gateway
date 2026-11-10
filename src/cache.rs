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
    /// 私有协议上游（Trae / WorkBuddy）的解密后凭据。
    /// 标准上游为 None——它们用 `platform.api_key`。
    ///
    /// **明文驻留内存的取舍**：与现有 `platforms.api_key` 同等级（那个也是明文常驻）。
    /// 每次请求都解一遍 AES 没必要，凭据变化是低频事件（刷新/用户修改），
    /// 刷新后 `cache.refresh()` 会重新解密。这里不落日志、不出 HTTP 响应。
    pub credential: Option<crate::db::credential::CredentialSecret>,
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
    /// 每次上游尝试前置 32~128 位随机字母数字串
    pub request_nonce: bool,
    /// 路由级参数覆盖（该虚拟模型下所有后端共用），见 `Route::param_overrides`
    pub param_overrides: serde_json::Value,
}

/// 请求热路径的只读快照。整份快照一次原子替换，绝不原地 clear：
/// 曾经的 clear→逐条 insert 之间存在空窗，`/api` 会话刷新时会让
/// `has_api_keys()==false` 瞬时放行所有请求，路由刷新则让在途请求误报 404。
/// 快照里存 Arc<ResolvedRoute>，读路径只克隆一个 Arc，不再深拷贝整条路由。
#[derive(Default)]
struct CacheSnapshot {
    /// proxy name → 已解析路由
    entries: std::collections::HashMap<String, Arc<ResolvedRoute>>,
    /// sha256(密钥明文) → 行（零 DB 查询验证，库里也没有明文可对）
    api_keys: std::collections::HashMap<String, StoredApiKey>,
}

/// 请求热路径的内存缓存，消除所有同步 DB 查询
#[derive(Clone)]
pub struct RouteCache {
    inner: Arc<RouteCacheInner>,
}

struct RouteCacheInner {
    /// 当前生效的只读快照；读锁极廉价，写只发生在管理操作后的原子替换
    snapshot: parking_lot::RwLock<Arc<CacheSnapshot>>,
    /// 串行化两种 refresh 的构建：否则并发刷新会以「完成顺序」覆盖 DB 的写入顺序，
    /// 也会互相把对方刚写的另一半快照覆盖掉。
    refresh_lock: parking_lot::Mutex<()>,
    /// DB 连接池（仅用于刷新缓存）
    db: DbPool,
    /// 凭据解密用的密钥（来自 config.toml 的 encrypt_key）。
    /// 用 RwLock 而不是快照：`PUT /api/settings` 可以改 encrypt_key，
    /// 改完必须能立刻解出用新密钥写入的凭据。
    encrypt_key: parking_lot::RwLock<String>,
    /// last_used 去抖：同一把 Key 在一个间隔内至多写一次 DB，避免每个请求
    /// 都 spawn 一次同步 UPDATE 抢占连接池与 SQLite 写锁。
    last_used: DashMap<String, std::time::Instant>,
}

/// 同一把 Key 的 last_used 最短落库间隔
const LAST_USED_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

impl RouteCache {
    /// 创建缓存并从 DB 加载初始数据
    pub fn new(db: DbPool, encrypt_key: String) -> Self {
        let cache = Self {
            inner: Arc::new(RouteCacheInner {
                snapshot: parking_lot::RwLock::new(Arc::new(CacheSnapshot::default())),
                refresh_lock: parking_lot::Mutex::new(()),
                db,
                encrypt_key: parking_lot::RwLock::new(encrypt_key),
                last_used: DashMap::new(),
            }),
        };
        // 初始加载
        cache.refresh();
        cache.refresh_api_keys();
        cache
    }

    /// 更新凭据解密密钥并重建缓存（settings 改了 encrypt_key 后调用）。
    pub fn set_encrypt_key(&self, key: String) {
        *self.inner.encrypt_key.write() = key;
        self.refresh();
    }

    /// 从 DB 重建整个路由表（在管理操作后调用）。
    /// 先在锁外把新快照完整构建好，最后一步才原子替换——读者要么看到旧快照，
    /// 要么看到新快照，永远不会看到「半张表」。
    pub fn refresh(&self) {
        let _refresh_guard = self.inner.refresh_lock.lock();
        let db = &self.inner.db;
        let proxies = match crate::db::proxy::list(db) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("刷新缓存失败（获取代理列表）: {}", e);
                return;
            }
        };

        let mut new_entries = std::collections::HashMap::with_capacity(proxies.len());
        let encrypt_key = self.inner.encrypt_key.read().clone();

        for proxy in &proxies {
            let route = match crate::db::route::get_by_proxy(db, &proxy.id) {
                Ok(Some(r)) => r,
                _ => continue,
            };

            let mut resolved_backends = Vec::with_capacity(route.backends.len());
            for backend in &route.backends {
                match crate::db::platform::get(db, &backend.platform_id) {
                    Ok(platform) => {
                        // 私有协议上游需要凭据；解密失败不静默当成「没配」——
                        // 那会让请求以一个看不懂的错误失败（比如空 token）。
                        let credential = if crate::models::adapter::PlatformAdapter::resolve(
                            platform.adapter.as_deref(),
                            &platform.platform_type,
                        )
                        .is_proprietary()
                        {
                            match crate::db::credential::get(db, &platform.id, &encrypt_key) {
                                Ok(found) => found.map(|stored| stored.secret),
                                Err(e) => {
                                    tracing::warn!(
                                        "平台 {} 的凭据解密失败（请求将因缺凭据而失败）: {}",
                                        platform.id,
                                        e
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        resolved_backends.push(ResolvedBackend {
                            backend: backend.clone(),
                            platform,
                            credential,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("跳过后端 {}（平台 {} 查询失败: {}）", backend.id, backend.platform_id, e);
                    }
                }
            }

            new_entries.insert(
                proxy.name.clone(),
                Arc::new(ResolvedRoute {
                    route_id: route.id,
                    proxy_id: route.proxy_id,
                    backends: resolved_backends,
                    lb_strategy: route.lb_strategy,
                    retry_policy: route.retry_policy,
                    fallback: route.fallback,
                    default_max_tokens: route.default_max_tokens,
                    request_nonce: route.request_nonce,
                    param_overrides: route.param_overrides,
                }),
            );
        }

        let count = new_entries.len();
        let snapshot = CacheSnapshot {
            entries: new_entries,
            // 保留另一半：两种 refresh 共用一份快照，不能把 API key 表抹掉
            api_keys: self.inner.snapshot.read().api_keys.clone(),
        };
        // 写锁内一次原子替换：读者要么看到旧快照，要么看到新快照
        *self.inner.snapshot.write() = Arc::new(snapshot);
        tracing::info!("路由缓存已刷新，共 {} 个代理", count);
    }

    /// 异步版刷新：把同步的 O(P×B) 查询 + 凭据解密搬到 blocking 线程池。
    /// 管理接口曾经直接在 async handler 里调 `refresh()`，一次配置变更就能把
    /// 一个 actix worker 钉在 DB 查询上，和同一 worker 上的代理请求一起变慢。
    pub async fn refresh_async(&self) -> Result<(), AppError> {
        let cache = self.clone();
        actix_web::web::block(move || cache.refresh())
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 异步版 API key 刷新（同上，避免在 worker 线程上跑 DB 查询）
    pub async fn refresh_api_keys_async(&self) -> Result<(), AppError> {
        let cache = self.clone();
        actix_web::web::block(move || cache.refresh_api_keys())
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 按代理名查找已解析的路由（零 DB 查询、零深拷贝）
    pub fn get_resolved_route(&self, proxy_name: &str) -> Option<Arc<ResolvedRoute>> {
        self.inner.snapshot.read().entries.get(proxy_name).cloned()
    }

    /// 刷新 API key 缓存（在 API key 增删改后调用）。同样先在锁外构建、再原子替换。
    pub fn refresh_api_keys(&self) {
        let _refresh_guard = self.inner.refresh_lock.lock();
        match crate::db::api_key::list(&self.inner.db) {
            Ok(keys) => {
                let mut api_keys = std::collections::HashMap::with_capacity(keys.len());
                for key in keys {
                    api_keys.insert(key.key_hash.clone(), key);
                }
                let count = api_keys.len();
                let snapshot = CacheSnapshot {
                    entries: self.inner.snapshot.read().entries.clone(),
                    api_keys,
                };
                *self.inner.snapshot.write() = Arc::new(snapshot);
                tracing::info!("API key 缓存已刷新，共 {} 个", count);
            }
            Err(e) => {
                tracing::error!("刷新 API key 缓存失败: {}", e);
            }
        }
    }

    /// 是否配置了任何 API key（未配置时兼容旧行为：允许访问）
    pub fn has_api_keys(&self) -> bool {
        !self.inner.snapshot.read().api_keys.is_empty()
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
        self.inner.snapshot.read().api_keys
            .get(&crate::db::api_key::hash_key(key))
            .cloned()
    }

    /// 检查 API key 是否有效（纯内存查询，零 DB 交互）
    pub fn validate_api_key(&self, key: &str) -> Result<bool, AppError> {
        // 如果没有任何 API key 配置，允许所有访问
        if !self.has_api_keys() {
            return Ok(true);
        }
        Ok(self.find_enabled_api_key(key).is_some())
    }

    /// 记录 API key 最后使用时间：按 Key 去抖，且同步 SQL 走 blocking 线程池。
    /// 曾经每个鉴权请求都 `tokio::spawn` 一次同步 UPDATE（span 落在 reactor 线程上），
    /// N 并发时既钉住 worker 又把 SQLite 写锁打满。
    pub fn touch_api_key(&self, api_key_id: &str) {
        let now = std::time::Instant::now();
        // DashMap 的分片锁顺便保证「同一把 Key 只有一个请求拿到写资格」
        let mut entry = self.inner.last_used
            .entry(api_key_id.to_string())
            .or_insert_with(|| now - LAST_USED_MIN_INTERVAL);
        if now.duration_since(*entry) < LAST_USED_MIN_INTERVAL {
            return;
        }
        *entry = now;
        drop(entry);

        let db = self.inner.db.clone();
        let id = api_key_id.to_string();
        tokio::task::spawn_blocking(move || {
            let _ = crate::db::api_key::update_last_used(&db, &id);
        });
    }
}

/// 单条通道容量。有界 + `try_send`：DB 写不动时宁可显式丢弃并计数，
/// 也不让 5 条无界队列（正文单条可达 512KB/1MB）无限膨胀把进程撑爆。
/// 曾经的 `unbounded_channel` 注释写着「满了就丢弃」，实际永不阻塞也永不丢。
const STATS_CHANNEL_CAPACITY: usize = 4096;

/// 后台统计写入器：通过 channel 接收统计数据，批量异步写入 DB。
/// 请求统计与重试轨迹共用同一个后台任务（同一条热路径、同样只写本地 SQLite）。
#[derive(Clone)]
pub struct StatsWriter {
    tx: mpsc::Sender<RequestStat>,
    attempt_tx: mpsc::Sender<RequestAttempt>,
    body_tx: mpsc::Sender<crate::models::stats::RequestBody>,
    headers_tx: mpsc::Sender<crate::models::stats::RequestHeaders>,
    response_tx: mpsc::Sender<crate::models::stats::ResponseBody>,
    dropped: Arc<std::sync::atomic::AtomicU64>,
}

impl StatsWriter {
    /// 启动后台写入任务，返回 StatsWriter 发送端。
    /// `hub` 只被后台任务用来在落库后通知控制台：统计是攒一批 500ms 才写的，
    /// 不广播就没人知道该刷新了；发送端本身不需要事件总线。
    pub fn start(db: DbPool, hub: crate::events::EventHub) -> Self {
        let (tx, mut rx) = mpsc::channel::<RequestStat>(STATS_CHANNEL_CAPACITY);
        let (attempt_tx, mut attempt_rx) = mpsc::channel::<RequestAttempt>(STATS_CHANNEL_CAPACITY);
        let (body_tx, mut body_rx) = mpsc::channel::<crate::models::stats::RequestBody>(STATS_CHANNEL_CAPACITY);
        let (headers_tx, mut headers_rx) = mpsc::channel::<crate::models::stats::RequestHeaders>(STATS_CHANNEL_CAPACITY);
        let (response_tx, mut response_rx) = mpsc::channel::<crate::models::stats::ResponseBody>(STATS_CHANNEL_CAPACITY);
        let events = hub;
        let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));

        tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(64);
            let mut attempt_buffer = Vec::with_capacity(64);
            let mut body_buffer = Vec::with_capacity(64);
            let mut headers_buffer = Vec::with_capacity(64);
            let mut response_buffer = Vec::with_capacity(64);
            // 任一通道关闭就进入「排空剩余 → 最后一次 flush」的收尾路径。
            // 曾经的实现一遇到第一个 None 就 flush 一次并 return，其余 4 个缓冲直接丢。
            let mut closing = false;
            loop {
                let timeout = tokio::time::sleep(std::time::Duration::from_millis(500));
                tokio::pin!(timeout);

                loop {
                    tokio::select! {
                        stat = rx.recv(), if !closing => {
                            match stat {
                                Some(s) => buffer.push(s),
                                None => closing = true,
                            }
                        }
                        attempt = attempt_rx.recv(), if !closing => {
                            match attempt {
                                Some(a) => attempt_buffer.push(a),
                                None => closing = true,
                            }
                        }
                        body = body_rx.recv(), if !closing => {
                            match body {
                                Some(b) => body_buffer.push(b),
                                None => closing = true,
                            }
                        }
                        headers = headers_rx.recv(), if !closing => {
                            match headers {
                                Some(h) => headers_buffer.push(h),
                                None => closing = true,
                            }
                        }
                        response = response_rx.recv(), if !closing => {
                            match response {
                                Some(r) => response_buffer.push(r),
                                None => closing = true,
                            }
                        }
                        _ = &mut timeout => {
                            break;
                        }
                    }
                    if closing {
                        // 把还留在通道里的残余全部取出来再走，避免关停时丢数据
                        Self::drain_remaining(&mut rx, &mut buffer);
                        Self::drain_remaining(&mut attempt_rx, &mut attempt_buffer);
                        Self::drain_remaining(&mut body_rx, &mut body_buffer);
                        Self::drain_remaining(&mut headers_rx, &mut headers_buffer);
                        Self::drain_remaining(&mut response_rx, &mut response_buffer);
                        break;
                    }
                }

                let written = buffer.len();
                let failed = buffer
                    .iter()
                    .filter(|s| s.status_code >= 400 || s.error_type.is_some())
                    .count();
                let batch = BatchBuffers {
                    stats: std::mem::take(&mut buffer),
                    attempts: std::mem::take(&mut attempt_buffer),
                    bodies: std::mem::take(&mut body_buffer),
                    headers: std::mem::take(&mut headers_buffer),
                    responses: std::mem::take(&mut response_buffer),
                };
                if !batch.is_empty() {
                    // 同步 SQL 搬到 blocking 线程池：不再占用 tokio/actix worker 线程
                    let db_for_flush = db.clone();
                    let _ = tokio::task::spawn_blocking(move || batch.flush(&db_for_flush)).await;
                }
                if written > 0 {
                    events.publish(crate::events::ConsoleEvent::Requests { count: written, failures: failed });
                }
                if closing {
                    return;
                }
            }
        });

        Self { tx, attempt_tx, body_tx, headers_tx, response_tx, dropped }
    }

    fn drain_remaining<T>(rx: &mut mpsc::Receiver<T>, out: &mut Vec<T>) {
        while let Ok(item) = rx.try_recv() {
            out.push(item);
        }
    }

    fn drop_count(&self) {
        self.dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 非阻塞发送统计数据：队列满就丢并计数，绝不阻塞请求
    pub fn record(&self, stat: RequestStat) {
        if self.tx.try_send(stat).is_err() {
            self.drop_count();
        }
    }

    /// 非阻塞发送一次尝试的轨迹；写失败只影响"能不能回溯重试过程"，不影响请求本身
    pub fn record_attempt(&self, attempt: RequestAttempt) {
        if self.attempt_tx.try_send(attempt).is_err() {
            self.drop_count();
        }
    }

    /// 非阻塞发送请求正文。只有采集开着时才会被调用（见 proxy::handler::capture_body）；
    /// 与统计一样绝不阻塞请求，也绝不在请求线程上写 SQL。
    pub fn record_body(&self, body: crate::models::stats::RequestBody) {
        if self.body_tx.try_send(body).is_err() {
            self.drop_count();
        }
    }

    /// 非阻塞发送请求头；与正文独立，长请求即使正文超限也能留下头。
    pub fn record_headers(&self, headers: crate::models::stats::RequestHeaders) {
        if self.headers_tx.try_send(headers).is_err() {
            self.drop_count();
        }
    }

    /// 非阻塞发送上游原始响应正文（已截断）。
    pub fn record_response_body(&self, response: crate::models::stats::ResponseBody) {
        if self.response_tx.try_send(response).is_err() {
            self.drop_count();
        }
    }
}

/// 一批待落库的数据：五张表共用一个事务。
struct BatchBuffers {
    stats: Vec<RequestStat>,
    attempts: Vec<RequestAttempt>,
    bodies: Vec<crate::models::stats::RequestBody>,
    headers: Vec<crate::models::stats::RequestHeaders>,
    responses: Vec<crate::models::stats::ResponseBody>,
}

impl BatchBuffers {
    fn is_empty(&self) -> bool {
        self.stats.is_empty()
            && self.attempts.is_empty()
            && self.bodies.is_empty()
            && self.headers.is_empty()
            && self.responses.is_empty()
    }

    fn flush(self, db: &DbPool) {
        if let Err(e) = crate::db::stats::record_batch(
            db,
            &self.stats,
            &self.attempts,
            &self.bodies,
            &self.headers,
            &self.responses,
        ) {
            tracing::warn!(
                "批量写入统计失败（{} 条统计 / {} 条轨迹 / {} 条正文）: {}",
                self.stats.len(),
                self.attempts.len(),
                self.bodies.len(),
                e
            );
        }
    }
}
