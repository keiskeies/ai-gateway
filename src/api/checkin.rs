use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::api::settings::SharedAppConfig;
use crate::checkin;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

fn internal(error: actix_web::error::BlockingError) -> AppError {
    AppError::Internal(error.to_string())
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub platform: String,
}

/// 已登录账号清单 + 定时任务。只有元信息，cookie 明文永远不出进程。
pub async fn list(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let (accounts, tasks) = web::block(move || {
        AppResult::Ok((checkin::accounts(&pool)?, checkin::tasks(&pool)?))
    })
    .await
    .map_err(internal)??;
    Ok(HttpResponse::Ok().json(json!({
        "accounts": accounts,
        "tasks": tasks,
        "platforms": checkin::capabilities(),
    })))
}

/// 定时任务的整体表单。`enabled` 省略时：新建按启用，编辑保持原状——
/// 界面上「改点位」和「开关任务」是两件事，不该因为少传一个字段就把停用悄悄打开。
#[derive(Deserialize)]
pub struct TaskBody {
    #[serde(default)]
    pub name: String,
    pub time: String,
    pub platform_ids: Vec<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

pub async fn create_task(
    db: web::Data<DbPool>,
    body: web::Json<TaskBody>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let input = body.into_inner();
    let view = web::block(move || {
        checkin::create_task(&pool, &input.name, &input.time, &input.platform_ids, input.enabled)
    })
    .await
    .map_err(internal)??;
    Ok(HttpResponse::Created().json(view))
}

pub async fn update_task(
    db: web::Data<DbPool>,
    path: web::Path<String>,
    body: web::Json<TaskBody>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let id = path.into_inner();
    let input = body.into_inner();
    let view = web::block(move || {
        checkin::update_task(
            &pool,
            &id,
            &input.name,
            &input.time,
            &input.platform_ids,
            input.enabled,
        )
    })
    .await
    .map_err(internal)??;
    Ok(HttpResponse::Ok().json(view))
}

pub async fn delete_task(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let id = path.into_inner();
    web::block(move || checkin::delete_task(&pool, &id))
        .await
        .map_err(internal)??;
    Ok(HttpResponse::NoContent().finish())
}

/// 立即运行一条任务（绕过账号级「今天已领 / 不足 3 小时」过滤），用来验证配置对不对。
pub async fn run_task_now(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let id = path.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let task = web::block({
        let pool = pool.clone();
        let id = id.clone();
        move || checkin::task_row(&pool, &id)
    })
    .await
    .map_err(internal)??;
    let message = checkin::run_task(&pool, &encrypt_key, &task, true).await?;
    let view = web::block({
        let pool = pool.clone();
        move || checkin::task_view(&pool, &id)
    })
    .await
    .map_err(internal)??;
    Ok(HttpResponse::Ok().json(json!({ "task": view, "message": message })))
}

/// 新建平台流程：只确定平台类型，登录结果等平台保存后再绑定。
pub async fn login_new(body: web::Json<LoginBody>) -> AppResult<HttpResponse> {
    let attempt_id = checkin::login_new(&body.platform)?;
    Ok(HttpResponse::Ok().json(json!({ "attempt_id": attempt_id })))
}

/// 已有平台登录 / 重新登录。
pub async fn login_platform(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let platform_id = path.into_inner();
    let attempt_id = web::block(move || checkin::login_platform(&pool, &platform_id))
        .await
        .map_err(internal)??;
    Ok(HttpResponse::Ok().json(json!({ "attempt_id": attempt_id })))
}

pub async fn attempt(path: web::Path<String>) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(checkin::attempt_view(&path.into_inner())?))
}

pub async fn run(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let platform_id = path.into_inner();
    let pool = db.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let view = checkin::run_once(&pool, &encrypt_key, &platform_id).await?;
    Ok(HttpResponse::Ok().json(view))
}

/// 顶部「一键登录/领取」的领取部分：所有已登录账号各跑一次。
pub async fn run_all(
    db: web::Data<DbPool>,
    config: web::Data<SharedAppConfig>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let encrypt_key = config.read().security.encrypt_key.clone();
    let views = checkin::run_all(&pool, &encrypt_key).await?;
    Ok(HttpResponse::Ok().json(json!({ "accounts": views })))
}

/// 退出登录：丢弃会话，平台本身保留。
pub async fn clear_session(
    db: web::Data<DbPool>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let pool = db.into_inner();
    let platform_id = path.into_inner();
    web::block(move || checkin::clear_session(&pool, &platform_id))
        .await
        .map_err(internal)??;
    Ok(HttpResponse::NoContent().finish())
}
