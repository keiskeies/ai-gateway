use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// `?resolved=1` 时把已解决的告警也带上（面板默认只列当前状态）
#[derive(Clone, Default, Deserialize)]
pub struct AlertQuery {
    pub resolved: Option<bool>,
}

/// 通知中心列表：先扫一遍当前状态，再返回合并后的行。
/// 扫描是读时行为（见 `db::alert` 的模块注释），所以这条接口也是"告警生成"的唯一入口；
/// 只有真的出现了新条件才发 SSE 信号，否则刷新一次就广播一次会变成自激震荡。
pub async fn list(
    db: web::Data<DbPool>,
    hub: web::Data<crate::events::EventHub>,
    config: web::Data<crate::api::settings::SharedAppConfig>,
    query: web::Query<AlertQuery>,
) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let db_path = config.read().resolved_db_path();
    drop(config);
    let include_resolved = query.into_inner().resolved.unwrap_or(false);
    let (items, sweep) = web::block(move || {
        // 备份时间从文件系统读，不进库：快照是不是新鲜，跟库里写了什么无关
        let latest = crate::db::backup::latest_snapshot_time(&db_path);
        let candidates = crate::db::alert::collect_candidates(&db, chrono::Utc::now(), latest)?;
        let sweep = crate::db::alert::sweep(&db, &candidates)?;
        Ok::<_, AppError>((crate::db::alert::list(&db, include_resolved)?, sweep))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    if sweep.opened > 0 {
        hub.publish(crate::events::ConsoleEvent::Alerts {
            opened: sweep.opened,
            unread: sweep.unread,
        });
    }
    Ok(HttpResponse::Ok().json(crate::models::alert::AlertList {
        items,
        unread: sweep.unread,
    }))
}

/// 标记单条已读。dedupe_key 是 `kind:<uuid>` 形式，路径参数按整段取。
pub async fn mark_read(db: web::Data<DbPool>, path: web::Path<String>) -> AppResult<HttpResponse> {
    let key = path.into_inner();
    let found = {
        let db = db.clone().into_inner();
        let k = key.clone();
        web::block(move || crate::db::alert::mark_read(&db, &k))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??
    };
    if !found {
        return Err(AppError::NotFound(format!("alert {}", key)));
    }
    let db = db.into_inner();
    let unread = web::block(move || crate::db::alert::unread_count(&db))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "unread": unread })))
}

pub async fn mark_all_read(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let marked = web::block(move || crate::db::alert::mark_all_read(&db))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "marked": marked, "unread": 0 })))
}
