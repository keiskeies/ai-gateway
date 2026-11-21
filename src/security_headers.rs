//! 控制台的安全响应头。
//!
//! 为什么要有这个模块：网关自己不发 TLS，控制台可能是 `http://` 直接跑在局域网里，
//! 那 CSP / `nosniff` 就是 XSS 与点击劫持唯一的止损线。
//! 之前一个都没有，等于任何一处注入都能直接带着 7 天的 `aigw_session` 干活。
//!
//! 只给静态与 `/api` 响应加头，`/v1/*` 原样放过：转发热路径的响应头一个字节都不动，
//! 免得给客户端（Claude Code / Codex 之类）凭空多出几行来。

use actix_web::body::MessageBody;
use actix_web::dev::ServiceResponse;
use actix_web::http::header::{
    HeaderValue, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use actix_web::middleware::Next;
use actix_web::{dev::ServiceRequest, Result};

/// 控制台 CSP。逐条说明为什么是这个形状，改之前先读完：
///
/// - `default-src 'self'`：兜底只认同源。`/v1` 与 `/api` 都在同源，够用。
///   局域网从 `http://192.168.1.20:1994` 打开时，同源就是那个地址，不需要同网段白名单。
/// - `script-src 'self' 'unsafe-inline' 'unsafe-eval'`：`index.html` 里那段防闪烁主题脚本
///   必须放行，所以带 `unsafe-inline`——**这是明知故犯**，去掉它首屏会闪一下白。
///   既然内联脚本拦不住，真正兜住注入的是 `connect-src`：注入出来的脚本即使跑起来，
///   也发不出跨源请求、把数据送到外站去。`unsafe-eval` 给 Vite dev 的 HMR 留着，
///   生产包不需要，但同一份头同时服务 dev 与 prod，取宽的那个。
/// - `frame-ancestors 'self'`：挡掉任意网站的点击劫持（恶意页面把控制台套进去骗你点），
///   同时允许同源套壳。**这里刻意不用 `'none'`**：大屏 `/wall` 就是给人投屏/嵌面板用的，
///   网关自己出一页把 `/wall` 框进去是合理用法。要嵌进**别的源**的页面（比如同网段的
///   Home Assistant）会被这条挡住——那是有意留的闸门，需要时按下面注释放开。
/// - `connect-src 'self' http://127.0.0.1:* http://localhost:*`：Tauri 桌面端在
///   `tauri://` 协议下会跨源探测后端端口（`api.ts` 里的端口发现），必须放行回环，
///   否则桌面端连不上自己的后端。
/// - `style-src ... 'unsafe-inline'`：Mantine 运行时注入 <style>，无解，社区通行做法。
/// - `img-src`/`font-src` 留 `data:`：构建产物里核过没有 data: 图片（tabler 图标是内联
///   SVG），但 Mantine 会内联字体，拦了就是白屏——这一项没有止损价值。
pub const CONSOLE_CSP: &str = "default-src 'self'; \
     base-uri 'self'; \
     form-action 'self'; \
     frame-ancestors 'self'; \
     object-src 'none'; \
     connect-src 'self' http://127.0.0.1:* http://localhost:*; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval'; \
     style-src 'self' 'unsafe-inline' 'unsafe-eval'; \
     img-src 'self' data: blob:; \
     font-src 'self' data:; \
     worker-src 'self' blob:";

/// 转发路径不加头：`/v1` 的响应是给 SDK 读的，多几行头没有安全收益，只有回归风险。
fn is_proxy_path(path: &str) -> bool {
    path.starts_with("/v1")
}

/// 给控制台与 `/api` 的响应补安全头。
///
/// CSP 用 `append` 而不是 `insert`：万一将来接了反向代理或 Tauri 自己已经发过一条
/// CSP，两条并存时浏览器取交集（更严），不会出现"后面那条把前面整条顶掉"的意外。
pub async fn security_headers<B: MessageBody>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<B>> {
    let proxy = is_proxy_path(req.path());
    let mut res = next.call(req).await?;
    if proxy {
        return Ok(res);
    }

    let headers = res.response_mut().headers_mut();
    if !headers.contains_key(CONTENT_SECURITY_POLICY) {
        headers.append(CONTENT_SECURITY_POLICY, HeaderValue::from_static(CONSOLE_CSP));
    }
    // **刻意不发 X-Frame-Options**：它只有 DENY / SAMEORIGIN 两档，而 SAMEORIGIN 与
    // CSP 的 `frame-ancestors 'self'` 同义——发它不会更严，反而会让"同源套壳嵌 /wall"
    // 这条合理用法变成白屏。点击劫持在 CSP 生效的浏览器上由 `frame-ancestors` 挡住，
    // 比 XFO 覆盖面更广（支持正则白名单、可精确到同源），所以这里只做减法。
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // 控制台 URL 里带虚拟模型名等标识，不给第三方页面带过去
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("same-origin"));

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::StatusCode, middleware, test as aw_test, web, App, HttpResponse};

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().body("ok")
    }

    /// 两个用例各自建 App：泛型 Service 类型写不出来，也不值得为测试引 actix-http 依赖。
    macro_rules! test_app {
        () => {
            aw_test::init_service(
                App::new()
                    .wrap(middleware::from_fn(security_headers))
                    .route("/index.html", web::get().to(ok))
                    .route("/api/platforms", web::get().to(ok))
                    .route("/v1/chat/completions", web::post().to(ok)),
            )
            .await
        };
    }

    #[actix_web::test]
    async fn console_and_api_responses_carry_headers() {
        let app = test_app!();
        for uri in ["/index.html", "/api/platforms"] {
            let resp =
                aw_test::call_service(&app, aw_test::TestRequest::get().uri(uri).to_request())
                    .await;
            assert_eq!(resp.status(), StatusCode::OK, "{uri}");
            let csp = resp
                .headers()
                .get(CONTENT_SECURITY_POLICY)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            assert!(
                csp.contains("frame-ancestors 'self'"),
                "{uri} 缺 CSP 或挡掉了同源套壳（大屏 /wall 要嵌自己）: {csp}"
            );
            // index.html 的内联主题脚本必须在白名单里，否则首屏脚本直接被拦
            assert!(csp.contains("script-src 'self' 'unsafe-inline'"), "{uri} 的 script-src 会打断首屏: {csp}");
            // XFO 缺席是**要守的**：它是 DENY/SAMEORIGIN 二选一，一发就把同源嵌 /wall 打成白屏，
            // 而点击劫持已经由 frame-ancestors 覆盖。哪天有人"顺手补上"这条，测试要响。
            assert!(
                !resp.headers().contains_key("X-Frame-Options"),
                "{uri} 不该带 X-Frame-Options（与 frame-ancestors 'self' 冲突）"
            );
            assert_eq!(
                resp.headers()
                    .get(X_CONTENT_TYPE_OPTIONS)
                    .and_then(|v| v.to_str().ok()),
                Some("nosniff"),
                "{uri}"
            );
        }
    }

    /// 转发路径必须一个头都不多——客户端 SDK 会把这些头回显进日志与排障截图里。
    #[actix_web::test]
    async fn proxy_path_is_left_alone() {
        let app = test_app!();
        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::post()
                .uri("/v1/chat/completions")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!resp.headers().contains_key(CONTENT_SECURITY_POLICY));
        assert!(!resp.headers().contains_key(X_CONTENT_TYPE_OPTIONS));
    }
}
