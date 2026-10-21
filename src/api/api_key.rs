use actix_web::{web, HttpResponse};
use rand::rngs::OsRng;
use rand::RngCore;
use crate::cache::RouteCache;
use crate::error::{AppError, AppResult};
use crate::db::DbPool;
use crate::models::api_key::*;
use std::sync::Arc;

fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("sk-aigw-{}", hex::encode(bytes))
}

/// 客户端自带 key 时用它，但太短的凭据不受理：
/// 库里只有摘要，创建后谁也想不起原值，一个 6 字符的 Key 就只能靠猜——那不该发出去。
const MIN_CUSTOM_KEY_LEN: usize = 20;

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let (keys, usage) = web::block(move || -> AppResult<(Vec<StoredApiKey>, std::collections::HashMap<String, KeyUsage>)> {
        Ok((crate::db::api_key::list(&db)?, crate::db::api_key::usage_by_key(&db)?))
    })
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    // 一律是掩码：列表刷新后不可能再取回完整 Key
    let public: Vec<ApiKey> = keys.iter().map(|key| key.to_public(usage.get(&key.id).copied().unwrap_or_default())).collect();
    Ok(HttpResponse::Ok().json(public))
}

/// 配额窗口只有三种滚动写法（加"累计"＝存 NULL）。认不出来就 400：
/// 悄悄当成"不限"会让一把设了预算的 Key 看起来有预算、实际谁也不拒。
fn normalize_window(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = raw else { return Ok(None) };
    match parse_quota_window(value) {
        WindowInput::Lifetime => Ok(None),
        WindowInput::Window(name) => Ok(Some(name)),
        WindowInput::Invalid => Err(AppError::BadRequest(format!(
            "Invalid quota_window; use {} or null for lifetime",
            QUOTA_WINDOWS.join(" / ")
        ))),
    }
}

/// 到期时间写错了不能默默当成"不过期"：列表上看着没限制、请求却随时可能被拒，
/// 那是最难查的一种故障。入口直接拒掉，并把接受下来的值统一成 RFC3339 UTC。
/// 归一化不是洁癖：裸日期 `2026-12-31` 判过期时是当天 23:59:59，若原样入库，
/// 列表按本地时区解析同一个串就成了 00:00，显示与限制差一整天。
fn normalize_expiry(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = raw.map(str::trim) else { return Ok(None) };
    if value.is_empty() {
        return Ok(None);
    }
    crate::models::api_key::parse_expiry(value)
        .map(|until| Ok(Some(until.to_rfc3339())))
        .unwrap_or_else(|| Err(AppError::BadRequest(
            "Invalid expires_at; use YYYY-MM-DD or RFC3339 (e.g. 2026-12-31T23:59:59Z)".to_string(),
        )))
}

pub async fn create(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    body: web::Json<CreateApiKeyRequest>,
) -> AppResult<HttpResponse> {
    let mut req = body.into_inner();
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("API key name is required".to_string()));
    }
    req.name = req.name.trim().to_string();
    if req.key.as_deref().map(str::trim).unwrap_or("").is_empty() {
        req.key = Some(generate_key());
    }
    if let Some(custom) = req.key.as_deref() {
        if custom.trim().chars().count() < MIN_CUSTOM_KEY_LEN {
            return Err(AppError::BadRequest(format!(
                "Custom key is too short ({} chars); use at least {MIN_CUSTOM_KEY_LEN}",
                custom.trim().chars().count()
            )));
        }
    }
    // 明文只在这一次响应里回显一次（secret），库里落的是摘要 + 掩码。
    req.expires_at = normalize_expiry(req.expires_at.as_deref())?;
    for (name, value) in [
        ("quota_requests", req.quota_requests),
        ("quota_tokens", req.quota_tokens),
    ] {
        if value.map(|v| v <= 0).unwrap_or(false) {
            return Err(AppError::BadRequest(format!("{name} must be a positive number or null")));
        }
    }
    if req.quota_cost.map(|v| v <= 0.0).unwrap_or(false) {
        return Err(AppError::BadRequest("quota_cost must be a positive number or null".to_string()));
    }
    req.quota_window = normalize_window(req.quota_window.as_deref())?;
    let secret = req.key.clone().unwrap_or_default();
    let db = db.into_inner();
    let stored = web::block(move || -> AppResult<StoredApiKey> {
        // proxy_id 不再有外键兜底：绑到不存在的模型只会得到一个永远无法鉴权的 Key，直接在入口拒掉
        if let Some(proxy_id) = req.proxy_id.as_deref() {
            if !crate::db::proxy::exists(&db, proxy_id)? {
                return Err(AppError::BadRequest("Bound virtual model does not exist".to_string()));
            }
        }
        crate::db::api_key::create(&db, &req)
    })
    .await.map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh_api_keys();
    let mut api_key = stored.to_public(KeyUsage::default());
    api_key.secret = Some(secret);
    Ok(HttpResponse::Created().json(api_key))
}

pub async fn update(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<UpdateApiKeyRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let mut req = body.into_inner();
    req.expires_at = match req.expires_at {
        NullableString::Set(value) => match normalize_expiry(Some(value.as_str()))? {
            // 空白串出现在 Set 里＝客户端想清空，按 Clear 处理，不能退成"没改"
            Some(until) => NullableString::Set(until),
            None => NullableString::Clear,
        },
        other => other,
    };
    req.quota_window = match req.quota_window {
        NullableString::Set(value) => match normalize_window(Some(value.as_str()))? {
            Some(name) => NullableString::Set(name),
            None => NullableString::Clear,
        },
        other => other,
    };
    let db = db.into_inner();
    let result = web::block(move || -> AppResult<Option<(StoredApiKey, KeyUsage)>> {
        // 改绑与新建同口径：绑到不存在的模型只会得到一把永远 401 的 Key
        if let NullableString::Set(proxy_id) = &req.proxy_id {
            if !crate::db::proxy::exists(&db, proxy_id)? {
                return Err(AppError::BadRequest("Bound virtual model does not exist".to_string()));
            }
        }
        let Some(updated) = crate::db::api_key::update(&db, &id, &req)? else {
            return Ok(None);
        };
        let usage = crate::db::api_key::usage_for_key(&db, &updated.id, updated.quota_window.as_deref())?;
        Ok(Some((updated, usage)))
    })
    .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let Some((api_key, usage)) = result else {
        return Err(AppError::NotFound("API key not found".to_string()));
    };
    // 过期/配额都活在缓存里：改完不刷一次，旧限制还能继续放行到下一轮
    cache.refresh_api_keys();
    Ok(HttpResponse::Ok().json(api_key.to_public(usage)))
}

/// 轮换密钥：路径里给 id，新值由后端生成并只在这一次响应里返回。
/// 缓存必须同步刷新，否则旧值在 TTL 内仍能通过鉴权。
pub async fn rotate(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let new_key = generate_key();
    let secret = new_key.clone();
    let db = db.into_inner();
    let rotated = web::block(move || -> AppResult<Option<(StoredApiKey, KeyUsage)>> {
        let Some(rotated) = crate::db::api_key::rotate(&db, &id, &new_key)? else {
            return Ok(None);
        };
        let usage = crate::db::api_key::usage_for_key(&db, &rotated.id, rotated.quota_window.as_deref())?;
        Ok(Some((rotated, usage)))
    })
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    let Some((stored, usage)) = rotated else {
        return Err(AppError::NotFound("API key not found".to_string()));
    };
    cache.refresh_api_keys();
    let mut api_key = stored.to_public(usage);
    api_key.secret = Some(secret);
    Ok(HttpResponse::Ok().json(api_key))
}

pub async fn delete(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || crate::db::api_key::delete(&db, &id))
        .await.map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh_api_keys();
    Ok(HttpResponse::NoContent().finish())
}

const BATCH_ACTIONS: [&str; 3] = ["enable", "disable", "delete"];
const MAX_BATCH_IDS: usize = 500;

/// 批量启用 / 停用 / 删除。回 `{affected, missing}` 而不是只回"成功"：
/// 界面上"已停用 12 个"必须对得上库里真的动了 12 行，别人已经删掉的 id 单独列出。
pub async fn batch(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    body: web::Json<BatchApiKeyRequest>,
) -> AppResult<HttpResponse> {
    let req = body.into_inner();
    if !BATCH_ACTIONS.contains(&req.action.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid action; use {}",
            BATCH_ACTIONS.join(" / ")
        )));
    }
    let mut ids: Vec<String> = Vec::new();
    for id in req.ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(AppError::BadRequest("At least one API key id is required".to_string()));
    }
    if ids.len() > MAX_BATCH_IDS {
        return Err(AppError::BadRequest(format!("At most {MAX_BATCH_IDS} ids per batch request")));
    }
    let action = req.action;
    let db = db.into_inner();
    let (affected, missing) = web::block(move || -> AppResult<(usize, Vec<String>)> {
        Ok(match action.as_str() {
            "enable" => crate::db::api_key::batch_set_enabled(&db, &ids, true)?,
            "disable" => crate::db::api_key::batch_set_enabled(&db, &ids, false)?,
            _ => crate::db::api_key::batch_delete(&db, &ids)?,
        })
    })
    .await.map_err(|e| AppError::Internal(e.to_string()))??;
    // 停用与删除都要立刻反映到鉴权：缓存不刷，旧状态还能放行到下一轮
    if affected > 0 {
        cache.refresh_api_keys();
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "affected": affected, "missing": missing })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 入库前归一化：裸日期是"当天结束"，而列表按同一串解析时必须落在同一个瞬间。
    #[test]
    fn expiry_is_stored_as_one_instant() {
        let until = normalize_expiry(Some("2026-12-31")).unwrap().unwrap();
        assert_eq!(until, "2026-12-31T23:59:59+00:00");
        assert!(crate::models::api_key::parse_expiry(&until).is_some(), "归一化后的值要能被自己再解析");
        assert_eq!(normalize_expiry(Some("  ")).unwrap(), None);
        assert_eq!(normalize_expiry(None).unwrap(), None);
        assert!(matches!(normalize_expiry(Some("tomorrow")), Err(AppError::BadRequest(_))));
    }

    /// 窗口写法归一：`lifetime` 折成 None（库里存 NULL），拼错的写法必须 400。
    /// 悄悄当成"不限"是最坏结果——一把设了预算的 Key 在界面上有预算、实际谁也不拒。
    #[test]
    fn quota_window_accepts_three_names_and_rejects_the_rest() {
        assert_eq!(normalize_window(None).unwrap(), None);
        assert_eq!(normalize_window(Some("lifetime")).unwrap(), None);
        assert_eq!(normalize_window(Some(" Month ")).unwrap().as_deref(), Some("month"));
        for bad in ["hour", "monthly", "yesterday"] {
            assert!(
                matches!(normalize_window(Some(bad)), Err(AppError::BadRequest(_))),
                "{bad} 不是窗口，不能当成不限"
            );
        }
    }
}
