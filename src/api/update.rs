//! 桌面端「检查更新」接口。
//!
//! 查询本身是只读的，但下载会写本机磁盘，所以整段和 `/api/clients`、`/api/checkin`
//! 一样挂在 `require_local_console` 后面；再加上 reveal opener 的判定，
//! 独立服务器模式下两个写接口都是 409。

use actix_web::{web, HttpResponse};

use crate::error::AppResult;
use crate::update;

#[derive(serde::Deserialize)]
pub struct CheckQuery {
    /// `1` 表示忽略缓存真的打一次接口
    pub force: Option<String>,
}

pub async fn check(query: web::Query<CheckQuery>) -> AppResult<HttpResponse> {
    let force = matches!(query.force.as_deref(), Some("1") | Some("true"));
    let info = update::check(force).await?;
    Ok(HttpResponse::Ok().json(info))
}

#[derive(serde::Deserialize)]
pub struct DownloadBody {
    pub asset: Option<String>,
}

pub async fn download(web::Json(body): web::Json<DownloadBody>) -> AppResult<HttpResponse> {
    let progress = update::start_download(body.asset).await?;
    Ok(HttpResponse::Accepted().json(progress))
}

pub async fn progress() -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(update::progress()))
}

pub async fn reveal() -> AppResult<HttpResponse> {
    update::reveal_installer()?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "ok": true })))
}
