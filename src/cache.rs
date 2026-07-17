use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::db::DbPool;
use crate::error::AppError;
use crate::models::api_key::ApiKey;
use crate::models::platform::Platform;
use crate::models::route::{Backend, LBStrategy, RetryPolicy};
use crate::models::stats::RequestStat;

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
    /// API key 缓存：key_string → ApiKey（零 DB 查询验证）
    api_keys: DashMap<String, ApiKey>,
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
                    self.inner.api_keys.insert(key.key.clone(), key);
                }
                tracing::info!("API key 缓存已刷新，共 {} 个", self.inner.api_keys.len());
            }
            Err(e) => {
                tracing::error!("刷新 API key 缓存失败: {}", e);
            }
        }
    }

    /// 检查 API key 是否有效（纯内存查询，零 DB 交互）
    pub fn validate_api_key(&self, key: &str) -> Result<bool, AppError> {
        // 如果没有任何 API key 配置，允许所有访问
        if self.inner.api_keys.is_empty() {
            return Ok(true);
        }
        Ok(self.inner.api_keys.contains_key(key))
    }

    /// 记录 API key 最后使用时间（异步发送，不阻塞请求）
    pub fn touch_api_key(&self, key: &str) {
        if let Some(ak) = self.inner.api_keys.get(key) {
            let db = self.inner.db.clone();
            let id = ak.id.clone();
            tokio::spawn(async move {
                let _ = crate::db::api_key::update_last_used(&db, &id);
            });
        }
    }
}

/// 后台统计写入器：通过 channel 接收统计数据，批量异步写入 DB
pub struct StatsWriter {
    tx: mpsc::UnboundedSender<RequestStat>,
}

impl StatsWriter {
    /// 启动后台写入任务，返回 StatsWriter 发送端
    pub fn start(db: DbPool) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<RequestStat>();

        tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(64);
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
                                    // channel 关闭，刷出剩余数据
                                    if !buffer.is_empty() {
                                        Self::flush(&db, &mut buffer);
                                    }
                                    return;
                                }
                            }
                        }
                        _ = &mut timeout => {
                            break;
                        }
                    }
                }

                if !buffer.is_empty() {
                    Self::flush(&db, &mut buffer);
                }
            }
        });

        Self { tx }
    }

    fn flush(db: &DbPool, buffer: &mut Vec<RequestStat>) {
        for stat in buffer.drain(..) {
            if let Err(e) = crate::db::stats::record(db, &stat) {
                tracing::warn!("写入统计数据失败: {}", e);
            }
        }
    }

    /// 非阻塞发送统计数据
    pub fn record(&self, stat: RequestStat) {
        // channel 满了就丢弃，不阻塞请求
        let _ = self.tx.send(stat);
    }
}
