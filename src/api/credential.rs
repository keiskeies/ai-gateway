//! 上游凭据的 CRUD 接口。
//!
//! 安全约定（与 `platforms.api_key` 同口径，但更严格）：
//! - 写接口接受明文，**响应永不回明文**
//! - 读接口只返回 `CredentialMeta`（kind / 到期时间 / 失败次数 / 是否自刷新）
//! - 明文只在 `db::credential::get` 返回给进程内调用方（转发链路）时出现

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::api::settings::SharedAppConfig;
use crate::cache::RouteCache;
use crate::db::credential::{CredentialKind, CredentialSecret};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct PutCredentialRequest {
    /// 直接给 token（最常用）
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub enterprise_id: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub machine_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    /// api_key / oauth；缺省时按「有 refresh_token 就是 oauth」推断
    #[serde(default)]
    pub kind: Option<String>,
    /// Trae 的 ExchangeToken 会轮换令牌族，默认关闭
    #[serde(default)]
    pub allow_self_refresh: Option<bool>,
}

impl PutCredentialRequest {
    fn into_secret(self) -> CredentialSecret {
        CredentialSecret {
            access_token: self.access_token.unwrap_or_default().trim().to_string(),
            refresh_token: self.refresh_token.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            uid: self.uid.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            enterprise_id: self
                .enterprise_id
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            domain: self.domain.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            device_id: self.device_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            machine_id: self.machine_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            expires_at: self.expires_at.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        }
    }
}

/// 读取凭据元信息。凭据不存在返回 404（而不是空对象——前端要能区分「没配」与「配了但读不到」）。
pub async fn get(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let stored = web::block(move || crate::db::credential::get(&db, &platform_id, &encrypt_key))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    match stored {
        Some(stored) => Ok(HttpResponse::Ok().json(stored.meta)),
        None => Err(AppError::NotFound("platform has no credential".to_string())),
    }
}

pub async fn put(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<PutCredentialRequest>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let req = body.into_inner();
    let allow_self_refresh = req.allow_self_refresh.unwrap_or(false);
    let explicit_kind = req.kind.clone();
    let secret = req.into_secret();
    let kind = match explicit_kind.as_deref() {
        Some(raw) => CredentialKind::parse(raw),
        None => {
            if secret.refresh_token.is_some() {
                CredentialKind::OAuth
            } else {
                CredentialKind::ApiKey
            }
        }
    };
    let db = db.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let pid = platform_id.clone();
    web::block(move || {
        // 平台必须存在，否则会留下悬空凭据。
        // 注意：`db::platform::get` 找不到行时返回的是 rusqlite 的
        // QueryReturnedNoRows（会被转成 500），这里必须显式转成 404——
        // 「平台不存在」是客户端错误，不是服务端故障。
        crate::db::platform::get(&db, &pid).map_err(|_| {
            AppError::NotFound(format!("platform not found: {pid}"))
        })?;
        crate::db::credential::upsert(&db, &pid, &secret, kind, allow_self_refresh, &encrypt_key)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::NoContent().finish())
}

pub async fn delete(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::credential::delete(&db, &platform_id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh();
    Ok(HttpResponse::NoContent().finish())
}

/// 手动刷新。当前实现只做「校验凭据还在、未过期」，真正的上游刷新在
/// `proxy::upstream` 的刷新任务里——这里不复制一份刷新逻辑，避免两处漂移。
pub async fn refresh(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let stored = web::block(move || crate::db::credential::get(&db, &platform_id, &encrypt_key))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    let Some(stored) = stored else {
        return Err(AppError::NotFound("platform has no credential".to_string()));
    };
    if stored.meta.expired {
        return Err(AppError::BadRequest(
            "credential already expired; re-import it from the official client".to_string(),
        ));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "expires_at": stored.meta.expires_at,
        "expired": stored.meta.expired,
    })))
}

/// Key 申请快捷入口注册表。只读，不含敏感信息。
pub async fn key_links() -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(crate::models::key_link::KeyLink::all()))
}

/// 查询上游权威额度并校准本地台账。
///
/// 这是「本地为准、上游校准」策略的校准入口：热路径走本地 `try_reserve`
/// （零网络），后台/手动通过这个接口把漂移纠回来。
///
/// 行为：
/// - `credit_unit = credits` 且上游给了数字 → 写 `calibrate` 流水并更新 `spent`
/// - 上游没给数字（无限额度/字段缺失）→ 只返回查询结果，不动台账
/// - 上游查询失败 → 502，但**不影响本地台账**（额度同步是增强功能）
pub async fn quota_sync(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let db = db.into_inner();
    let (encrypt_key, timeout_secs) = {
        let cfg = config.read();
        (
            cfg.security.encrypt_key.clone(),
            cfg.defaults.test_connection_timeout_secs,
        )
    };

    // 读平台 + 凭据。db 要 clone：下面校准还要再用一次。
    let pid = platform_id.clone();
    let db_for_read = db.clone();
    let (platform, credential) = web::block(move || {
        let platform = crate::db::platform::get(&db_for_read, &pid)?;
        let stored = crate::db::credential::get(&db_for_read, &pid, &encrypt_key)?;
        Ok::<_, crate::error::AppError>((platform, stored.map(|s| s.secret)))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let adapter = crate::models::adapter::PlatformAdapter::resolve(
        platform.adapter.as_deref(),
        &platform.platform_type,
    );
    if !adapter.is_proprietary() {
        return Err(AppError::BadRequest(format!(
            "{} 没有上游额度接口；额度由模型价格本地计算",
            adapter.label()
        )));
    }
    let Some(credential) = credential else {
        return Err(AppError::NotFound(
            "platform has no credential; import one before syncing quota".to_string(),
        ));
    };

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let quota = crate::proxy::upstream::quota::query(
        &client,
        adapter,
        &platform,
        &credential,
        timeout_secs,
    )
    .await
    .map_err(|e| {
        // 上游额度查询失败：回 502 语义。AppError 没有 BadGateway 变体，
        // 用 Internal 会变成 500；这里显式构造一个 502 响应更准确。
        AppError::Internal(format!("上游额度查询失败: {e}"))
    })?;

    // 只有 credits 台账 + 上游给了数字时才校准
    let mut calibrated = None;
    let unit = crate::models::adapter::CreditUnit::resolve(platform.credit_unit.as_deref());
    if unit.uses_credit_ledger() {
        if let (Some(remaining), Some(budget), Some(window)) = (
            quota.remaining,
            platform.credit_budget,
            platform.credit_window.as_deref(),
        ) {
            let db2 = db.clone();
            let pid2 = platform_id.clone();
            let window = window.to_string();
            calibrated = Some(
                web::block(move || {
                    crate::db::credit::calibrate(&db2, &pid2, budget, &window, remaining)
                })
                .await
                .map_err(|e| AppError::Internal(e.to_string()))??,
            );
        }
    }
    cache.refresh();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "adapter": format!("{adapter:?}"),
        "credit_unit": unit.key_str(),
        "upstream_remaining": quota.remaining,
        "upstream_total": quota.total,
        // (old_spent, new_spent, delta)；None = 未校准（单位不是 credits 或上游没给数字）
        "calibrated": calibrated.map(|(old, new, delta)| serde_json::json!({
            "old_spent": old,
            "new_spent": new,
            "delta": delta,
        })),
        "raw": quota.raw,
    })))
}

/// 按预设名 / base_url 反查一个入口，供创建弹窗在切换预设时即时显示按钮。
pub async fn key_link_for(
    query: web::Query<KeyLinkQuery>,
) -> AppResult<HttpResponse> {
    let hit = crate::models::key_link::KeyLink::find_for(
        query.preset.as_deref(),
        query.base_url.as_deref(),
    );
    match hit {
        Some(link) => Ok(HttpResponse::Ok().json(link)),
        None => Err(AppError::NotFound("no key link for this platform".to_string())),
    }
}

#[derive(Debug, Deserialize)]
pub struct KeyLinkQuery {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}
