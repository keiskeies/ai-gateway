use actix_web::{web, HttpResponse};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::api::settings::SharedAppConfig;
use crate::cache::RouteCache;
use crate::clients::{service, ClientApplyRequest, ClientId, ClientPaths, ConfigFileBody};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

fn parse_id(path: web::Path<String>) -> AppResult<ClientId> {
    ClientId::from_str(&path.into_inner())
}

fn client_paths() -> AppResult<ClientPaths> {
    ClientPaths::from_env()
}

pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let paths = client_paths()?;
    let db = db.into_inner();
    let statuses = web::block(move || service::list(&db, &paths))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    Ok(HttpResponse::Ok().json(statuses))
}

pub async fn preview(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    body: web::Json<ClientApplyRequest>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let admin_port = settings.read().server.admin_port;
    let request = body.into_inner();
    let db = db.into_inner();
    let preview = web::block(move || service::preview(&db, &paths, id, &request, admin_port))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    Ok(HttpResponse::Ok().json(preview))
}

pub async fn apply(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    body: web::Json<ClientApplyRequest>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let request = body.into_inner();
    let (admin_port, backup_root) = {
        let config = settings.read();
        (
            config.server.admin_port,
            config
                .resolved_db_path()
                .parent()
                .unwrap_or(Path::new("."))
                .join("client-backups"),
        )
    };
    let db = db.into_inner();
    let outcome = web::block(move || {
        service::apply(&db, &paths, id, &request, admin_port, &backup_root)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))??;
    // 新建/停用 Key 后必须立刻刷新鉴权缓存，否则旧 Key 还能放行到下一轮。
    if outcome.cache_refresh {
        cache.refresh_api_keys();
    }
    Ok(HttpResponse::Ok().json(outcome.result))
}

pub async fn restore(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let backup_root = backup_root(&settings);
    let db = db.into_inner();
    let outcome = web::block(move || service::restore(&db, &paths, id, &backup_root))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    if outcome.cache_refresh {
        cache.refresh_api_keys();
    }
    Ok(HttpResponse::Ok().json(outcome.result))
}

pub async fn restore_backup(
    db: web::Data<DbPool>,
    cache: web::Data<Arc<RouteCache>>,
    path: web::Path<String>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let backup_root = backup_root(&settings);
    let db = db.into_inner();
    let outcome = web::block(move || service::restore_backup(&db, &paths, id, &backup_root))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    if outcome.cache_refresh {
        cache.refresh_api_keys();
    }
    Ok(HttpResponse::Ok().json(outcome.result))
}

/// 读配置文件原文（逃生口：白名单之外还有大量客户端自己的键）
pub async fn read_config(
    path: web::Path<String>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let _settings = settings;
    let file = web::block(move || service::read_config_file(&paths, id))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    Ok(HttpResponse::Ok().json(file))
}

/// 直接写配置文件：先校验这个客户端还读得懂，再备份、再写。
pub async fn write_config(
    path: web::Path<String>,
    body: web::Json<ConfigFileBody>,
    settings: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let id = parse_id(path)?;
    let paths = client_paths()?;
    let backup_root = backup_root(&settings);
    let content = body.into_inner().content;
    let result = web::block(move || service::write_config_file(&paths, &backup_root, id, &content))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    Ok(HttpResponse::Ok().json(result))
}

fn backup_root(settings: &SharedAppConfig) -> PathBuf {
    let config = settings.read();
    config
        .resolved_db_path()
        .parent()
        .unwrap_or(Path::new("."))
        .join("client-backups")
}
