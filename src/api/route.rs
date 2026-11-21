use actix_web::{web, HttpResponse};
use crate::cache::RouteCache;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::route::*;
use std::sync::Arc;

macro_rules! db_block {
    ($db:expr, $f:expr) => {
        web::block(move || $f).await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??
    };
}

/// 校验并归一化虚拟模型级的思考等级配置（create / update 共用）。
///
/// 归一化：trim + 转小写 + 去空 + 去重，和 `ReasoningEffort::parse` 的口径一致，
/// 免得 `"High"` 和 `"high"` 在库里变成两个档位。
///
/// 两条硬约束（前端界面已经保证，这里再挡一遍直连 API 的写法）：
/// - 默认等级必须是对外暴露列表的成员 —— 否则 `/v1/models` 报的档位和实际注入的对不上；
/// - 默认等级不能是 `none`/`disabled`：「默认关闭」和「请求没带时补一个等级」语义相反，
///   `none` 作为列表成员是合法的（表示「客户端可以关」），只是不能当默认值。
///
/// 返回归一化后的 `(efforts, default_effort)`；`declared` 为 false（未设置列表）时不校验默认值，
/// 保持「没配置过的模型行为一字不变」。
fn normalize_efforts(
    efforts: &[String],
    default: Option<&str>,
) -> AppResult<(Vec<String>, Option<String>)> {
    use crate::protocol::reasoning::ReasoningEffort;

    let mut list: Vec<String> = Vec::with_capacity(efforts.len());
    for raw in efforts {
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty() || list.contains(&value) {
            continue;
        }
        if value.len() > 32 || value.chars().any(char::is_whitespace) {
            return Err(crate::error::AppError::BadRequest(format!(
                "思考等级 \"{}\" 不合法：不能含空格且不超过 32 个字符",
                raw.trim()
            )));
        }
        list.push(value);
    }
    if list.len() > 16 {
        return Err(crate::error::AppError::BadRequest(
            "思考等级最多 16 档".into(),
        ));
    }

    let default = default.map(str::trim).filter(|d| !d.is_empty()).map(|d| d.to_ascii_lowercase());
    if let Some(d) = &default {
        if !list.iter().any(|e| e == d) {
            return Err(crate::error::AppError::BadRequest(format!(
                "默认思考等级 \"{}\" 必须先在暴露列表里勾上",
                d
            )));
        }
        if matches!(ReasoningEffort::parse(d), Some(ReasoningEffort::None)) {
            return Err(crate::error::AppError::BadRequest(
                "默认思考等级不能是 none（等于「默认关闭思考」，与「未带参数时补一个等级」相反）".into(),
            ));
        }
    }
    Ok((list, default))
}

/// 全部路由 + 后端（一次请求），供虚拟模型列表页替代逐个拉取
pub async fn list_all(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let routes = db_block!(db, crate::db::route::list_all(&db));
    Ok(HttpResponse::Ok().json(routes))
}

pub async fn list(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let db = db.into_inner();
    let routes = db_block!(db, crate::db::route::list_by_proxy(&db, &proxy_id));
    Ok(HttpResponse::Ok().json(routes))
}

pub async fn create(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<CreateRouteRequest>,
) -> AppResult<HttpResponse> {
    let proxy_id = path.into_inner();
    let mut req = body.into_inner();
    let (efforts, default) = normalize_efforts(&req.reasoning_efforts, req.default_effort.as_deref())?;
    req.reasoning_efforts = efforts;
    req.default_effort = default;
    let db = db.into_inner();
    let route = db_block!(db, crate::db::route::create(&db, &proxy_id, &req));
    cache.refresh_async().await?;
    Ok(HttpResponse::Created().json(route))
}

pub async fn update(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<UpdateRouteRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let mut req = body.into_inner();
    // 只想改默认值时，校验基准是「这次要落库的那份列表」：请求带了新列表就用新的，
    // 否则用库里现有的——否则「先勾 low/high 再设默认 high」能过、「只改默认 high」会被误拒。
    if req.reasoning_efforts.is_some() || req.default_effort.is_some() {
        let current = {
            let db2 = db.clone();
            let id2 = id.clone();
            db_block!(db2, crate::db::route::get(&db2, &id2))
        };
        let efforts_in = req
            .reasoning_efforts
            .as_ref()
            .unwrap_or(&current.reasoning_efforts);
        // 协议：default_effort 传空串 = 清空（落库为 NULL），不传 = 维持原值。
        // 清空不校验；维持原值时也要拿原值来对列表，否则「只改列表把旧默认值挤出去」能存进来。
        let default_in: Option<&str> = match req.default_effort.as_deref() {
            None => current.default_effort.as_deref(),
            Some(d) => Some(d),
        };
        let (efforts, default) = normalize_efforts(efforts_in, default_in)?;
        if req.reasoning_efforts.is_some() {
            req.reasoning_efforts = Some(efforts);
        }
        if let Some(raw) = &req.default_effort {
            let clearing = raw.trim().is_empty();
            // 归一化把「清空」折成 None，回写时要还原成 ""，否则 db::route::update
            // 会把它当成 None（= 这次没动这个字段）而留着旧默认值。
            req.default_effort = Some(if clearing { String::new() } else { default.unwrap_or_default() });
        }
    }
    let db = db.into_inner();
    let route = db_block!(db, crate::db::route::update(&db, &id, &req));
    cache.refresh_async().await?;
    Ok(HttpResponse::Ok().json(route))
}

pub async fn delete(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    db_block!(db, crate::db::route::delete(&db, &id));
    cache.refresh_async().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn list_backends(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let route_id = path.into_inner();
    let db = db.into_inner();
    let backends = db_block!(db, crate::db::route::list_backends_by_route(&db, &route_id));
    Ok(HttpResponse::Ok().json(backends))
}

pub async fn add_backend(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<CreateBackendRequest>,
) -> AppResult<HttpResponse> {
    let route_id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let backend = db_block!(db, crate::db::route::add_backend(&db, &route_id, &req));
    cache.refresh_async().await?;
    Ok(HttpResponse::Created().json(backend))
}

pub async fn update_backend(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<UpdateBackendRequest>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let backend = db_block!(db, crate::db::route::update_backend(&db, &id, &req));
    cache.refresh_async().await?;
    Ok(HttpResponse::Ok().json(backend))
}

/// 批量重排后端次序（主干拖拽）。逐条 PUT 会写出半套 priority，也会把缓存重建 N 次。
pub async fn reorder_backends(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<ReorderBackendsRequest>,
) -> AppResult<HttpResponse> {
    let route_id = path.into_inner();
    let req = body.into_inner();
    let db = db.into_inner();
    let backends = db_block!(db, crate::db::route::set_order(&db, &route_id, &req.backend_ids));
    cache.refresh_async().await?;
    Ok(HttpResponse::Ok().json(backends))
}

pub async fn delete_backend(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let db = db.into_inner();
    web::block(move || -> crate::error::AppResult<()> {
        crate::db::route::delete_backend(&db, &id)?;
        // 悬挂统计兜底：把历史统计中该 backend 的 backend_id 置空，保留请求数/token
        let _ = crate::db::stats::orphan_backend_stats(&db, &id);
        Ok(())
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    cache.refresh_async().await?;
    Ok(HttpResponse::NoContent().finish())
}
