//! 整库快照的 HTTP 面
//!
//! 全部走 `web::block`：`VACUUM INTO` 和在线还原都要持写锁跑完，放在异步线程上会把
//! actix 的工作线程占死；对单用户桌面应用来说，等待期间界面转圈是可以接受的。

use std::sync::Arc;

use actix_web::{web, HttpResponse};

use crate::api::settings::SharedAppConfig;
use crate::cache::RouteCache;
use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};

/// 快照目录跟着数据库文件走，所以路径每次现算：设置页改过数据目录后不该把备份写进老地方
fn active_db_path(config: &SharedAppConfig) -> std::path::PathBuf {
    config.read().resolved_db_path()
}

/// 只读目录，用不到连接池
pub async fn list(config: web::Data<SharedAppConfig>) -> AppResult<HttpResponse> {
    let path = active_db_path(&config);
    let result = web::block(move || db::backup::list(&path))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(result?))
}

/// 手动拍一份。没有定时任务，是刻意的：这台机器上"什么时候值得留一份"只有人知道。
pub async fn create(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let path = active_db_path(&config);
    let db = db.into_inner();
    let info = web::block(move || db::backup::create(&db, &path))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(info?))
}

pub async fn download(
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let name = path.into_inner();
    let db_path = active_db_path(&config);
    let for_fs = name.clone();
    let bytes = web::block(move || {
        let file = db::backup::path_for(&db_path, &for_fs)?;
        Ok::<_, AppError>(std::fs::read(&file)?)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .insert_header(("Content-Disposition", format!("attachment; filename=\"{name}\"")))
        .body(bytes))
}

pub async fn delete(
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let name = path.into_inner();
    let db_path = active_db_path(&config);
    let for_fs = name.clone();
    let removed = web::block(move || db::backup::delete(&db_path, &for_fs))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    if removed {
        Ok(HttpResponse::NoContent().finish())
    } else {
        Err(AppError::NotFound(format!("backup {name}")))
    }
}

/// 原地还原。成功后必须刷新路由与 Key 缓存，否则转发的还是还原前那套配置。
pub async fn restore(
    req: actix_web::HttpRequest,
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let name = path.into_inner();
    let db_path = active_db_path(&config);
    let db2 = db.clone().into_inner();
    let result = web::block(move || db::backup::restore(&db2, &db_path, &name))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    cache.refresh();
    cache.refresh_api_keys();
    // 会话行也在被还原的那份库里，留着这条 Cookie 只会让人下一次请求撞上一个说不清的 401
    crate::api::auth::drop_session(&req, &db).await?;
    Ok(HttpResponse::Ok()
        .cookie(crate::api::auth::clear_session_cookie())
        .json(result))
}
