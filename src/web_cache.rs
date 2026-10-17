//! 静态资源缓存策略中间件
//!
//! 背景：actix-files 默认只发送 `ETag` / `Last-Modified`，不发送 `Cache-Control`。
//! 浏览器（含 Tauri 使用的 WebView2）在缺少 `Cache-Control` 时会启用「启发式新鲜期」，
//! 新鲜期约为 `(Date - Last-Modified) * 0.1`。前端 `index.html` 若距上次修改已数月，
//! 会被判定为「未来数天内仍然新鲜」，客户端因此根本不回头校验，
//! 表现为「前端已经改了，但页面仍在跑旧 JS/CSS」。
//!
//! 策略：
//! - `/assets/*`：文件名带内容 hash（如 `index-Cb3hIslu.js`），内容变则文件名变，可永久强缓存。
//! - 其余静态路径（含 `/` 返回的 `index.html`）：`no-cache`，每次都必须回源校验，
//!   保证前端发布后立即生效（配合 ETag 仍可命中 304，代价很小）。
//! - API / 代理 / 健康检查路径不改写，避免影响 SSE 流式响应与既有 CORS 行为。

use actix_web::dev::ServiceResponse;
use actix_web::http::header::{CACHE_CONTROL, HeaderValue};
use actix_web::middleware::Next;
use actix_web::{dev::ServiceRequest, Result};

/// 带内容 hash 的静态资源前缀，可长期强缓存
const HASHED_ASSET_PREFIX: &str = "/assets/";

/// 判断是否属于不应改写缓存头的动态路径（API、代理端点、健康检查）
fn is_dynamic_path(path: &str) -> bool {
    path.starts_with("/api") || path.starts_with("/v1") || path == "/health"
}

/// 为静态响应补充 `Cache-Control` 头
///
/// `B` 由下游服务（actix-files）的 body 类型决定，这里用泛型让编译器自行推断。
pub async fn cache_headers<B>(req: ServiceRequest, next: Next<B>) -> Result<ServiceResponse<B>> {
    let path = req.path().to_string();
    let mut res = next.call(req).await?;

    if is_dynamic_path(&path) {
        return Ok(res);
    }

    let value = if path.starts_with(HASHED_ASSET_PREFIX) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    res.response_mut()
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(value));

    Ok(res)
}
