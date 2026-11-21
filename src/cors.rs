//! 控制台与转发端点的 CORS 策略。
//!
//! 为什么要换掉 `Cors::permissive()`：它会**同时**允许任意 origin 并打开
//! `supports_credentials`（actix-cors 0.7 源码 `builder.rs::permissive`，crate 文档自己
//! 写着 "should not be used in production"）。`aigw_session` 是 HttpOnly + `SameSite=Strict`
//! 的 7 天 cookie，Strict 已经挡住了**跨站**请求（外站页面带不走会话），所以别把这里当成
//! "之前随便一个网页都能冒充你"。真正的洞在**同站不同端口**：同源判定里 `localhost` 的
//! 所有端口算同一站，SameSite 一律放行，而 `permissive()` 又把 ACAO 与 credentials 都打开——
//! 于是本机任何起个端口的东西（别人装的那个 AI 工具的 web UI、某个 dev server）
//! 都能带着管理员会话读写 `/api/*`。局域网机器上这类本机进程很常见，这是本文件存在的理由。
//!
//! 改成什么：只放行回环与 Tauri 的 origin。
//!
//! **局域网使用不受影响**，因为从 `http://192.168.1.20:1994` 打开控制台后，
//! 前端所有请求都是相对路径（`api.ts` 里 baseURL 就是 `/api`），属于同源请求，
//! 浏览器根本不走 CORS、也不看这些响应头。CORS 只在"网页 A 请求 B 域"时才起作用，
//! 而手机、电视、别人的电脑打开这个控制台，都还是同源。
//!
//! 真正被挡掉的只有：装在别处的网页版工具想跨源调 `/api`。这类需求请走 `/v1`
//! （用 API Key 而不是会话，本来就该由 Key 而不是浏览器同源策略来控权限），
//! 或者干脆同机反代。

use actix_cors::Cors;

/// 判断一个 `Origin` 头的值是否属于本机：回环主机（任意端口）或 Tauri 的自定义协议。
///
/// Tauri 那几条是给桌面端的端口发现留的——`api.ts` 在仍停留在 `tauri://` 协议时会
/// 跨源 fetch `http://127.0.0.1:1994/api/auth/status`。现在的路径是 webview 直接导航到
/// `http://127.0.0.1:{port}`（同源，不需要 CORS），这条属于兜底路径，但兜底路径断了
/// 一样是白屏，所以留着。
pub fn is_local_origin(origin: &str) -> bool {
    let rest = match origin.split_once("://") {
        Some(("http", rest)) | Some(("https", rest)) => rest,
        // Tauri 在 macOS/Linux 用 tauri://localhost，Windows 上是 http(s)://tauri.localhost
        Some(("tauri", host)) => return matches!(host, "localhost" | ""),
        _ => return false,
    };
    match host_of(rest) {
        "127.0.0.1" | "localhost" | "::1" | "tauri.localhost" => true,
        // 整个 127/8 都是回环，不只是 .1：Origin 里出现 `127.0.0.1.evil.example` 这类
        // 伪前缀时 parse 会失败，所以判定必须建立在 parse 上而不是 starts_with 上。
        host => host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false),
    }
}

/// 从 `Origin` 的 authority 里取主机名，去掉端口。浏览器对 IPv6 一定带方括号。
fn host_of(authority: &str) -> &str {
    if let Some(after) = authority.strip_prefix('[') {
        return after.split(']').next().unwrap_or(after);
    }
    // 冒号后面全是数字才当端口切，否则是裸 IPv6（不认识的形态一律按不匹配处理）
    match authority.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    }
}

/// 控制台（`/api` 与静态资源）用的 CORS：只认同源与本机 origin。
///
/// 同源请求不经过这里（浏览器不发预检、不看 ACAO），所以实际效果就是"只有本机来源的
/// 网页能跨源读 `/api`"。
pub fn console_cors() -> Cors {
    // 不 allow_credentials：/api 的鉴权靠 HttpOnly 会话 cookie，
    // 跨源带 cookie 是我们想关掉的能力，而不是要精细放行的能力。
    // 上面 is_local_origin 放行回环与 tauri，是给 Tauri 端口发现那种
    // 「跨源但要读公开接口」的场景，它不需要 cookie。
    //
    // 方法和请求头必须显式放开：`Cors::default()` 是 crate 里那个 "security paranoid"
    // 预设，allowed_methods / allowed_headers 都是**空集**（builder.rs::default），
    // 只配 origin 的话，预检里带了方法或自定义头就一律 400 —— 也就是"origin 白名单写好了，
    // 但跨源请求一个都发不出来"。安全边界在 origin 白名单和没有 credentials 上：
    // 走到这里的方法/头只会回给已通过 is_local_origin 的来源，外站连的是上面那一关。
    let mut cors = Cors::default()
        .allow_any_method()
        .allow_any_header();
    // allowed_origin_fn 给的是 &HeaderValue（Origin 头原值），非 ASCII 时 to_str 会失败——
    // 那种值本来就不是合法 origin，按不放行处理。
    cors = cors.allowed_origin_fn(|origin, _ctx| {
        origin
            .to_str()
            .map(is_local_origin)
            .unwrap_or(false)
    });
    // 预检结果缓存 1 小时：控制台一次页面加载会打十几个 /api，别每次都预检
    cors.max_age(3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test as aw_test, App};

    #[test]
    fn loopback_and_tauri_origins_allowed() {
        for origin in [
            "http://127.0.0.1:1994",
            "http://127.0.0.1",
            "http://localhost:5173",
            "http://[::1]:1994",
            "http://127.0.0.53:1994",
            "tauri://localhost",
            "http://tauri.localhost",
        ] {
            assert!(is_local_origin(origin), "{origin} 应当放行（桌面端/本机开发要用）");
        }
    }

    /// 这条是这次改动的全部意义：任何外站（以及局域网里另一台机器的网页）
    /// 都不能跨源带着会话读 /api。
    #[test]
    fn remote_and_lan_origins_rejected() {
        for origin in [
            "https://evil.example.com",
            "http://192.168.1.20:1994",
            "http://10.0.0.5:3000",
            "http://127.0.0.1.evil.example",
            "http://evil.example/redirect?to=http://127.0.0.1:1994",
            "",
            "null",
        ] {
            assert!(!is_local_origin(origin), "{origin} 不该被放行");
        }
    }

    /// 只验证构造不 panic：actix-cors 在中间件初始化阶段才校验规则，
    /// 通配符 + credentials 这类非法组合会在这里报错，而我们没用通配符。
    #[test]
    fn console_cors_builds() {
        let _ = console_cors();
    }

    /// 预检真的能过。
    ///
    /// 这条是被真实事故逼出来的：`Cors::default()` 的方法表与请求头表都是空集，
    /// 只配 origin 时预检直接 400，白名单形同虚设。空集这种坑只有把一次真正的
    /// OPTIONS 打过去才看得见，所以这里不满足于"构造成功"。
    #[actix_web::test]
    async fn preflight_from_loopback_origin_succeeds() {
        use actix_web::{http::header, http::Method, web};

        async fn ok() -> &'static str {
            "ok"
        }
        let app = aw_test::init_service(
            App::new()
                .wrap(console_cors())
                // 预检被 Cors 中间件短路，走不到 handler；这条路由只要存在即可。
                .route("/api/ping", web::route().method(Method::POST).to(ok)),
        )
        .await;

        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::default()
                .method(Method::OPTIONS)
                .uri("/api/ping")
                .insert_header((header::ORIGIN, "http://127.0.0.1:1994"))
                .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "POST"))
                .insert_header((header::ACCESS_CONTROL_REQUEST_HEADERS, "Content-Type"))
                .to_request(),
        )
        .await;
        // actix-cors 预检成功回 200（不是 204），并把请求的方法与头回显出来
        assert!(resp.status().is_success(), "预检必须过: {}", resp.status());
        assert_eq!(
            resp.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "http://127.0.0.1:1994"
        );
        assert!(
            !resp.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            "开了 credentials 就等于把会话 Cookie 交给跨源页面"
        );
    }

    /// 外站的预检必须在第一关就被打掉——与上面那条配对，证明"放开方法与头"
    /// 没有把闸门一起放开。
    #[actix_web::test]
    async fn preflight_from_remote_origin_is_denied() {
        use actix_web::{http::header, http::Method, web};

        async fn ok() -> &'static str {
            "ok"
        }
        let app = aw_test::init_service(
            App::new()
                .wrap(console_cors())
                .route("/api/ping", web::get().to(ok)),
        )
        .await;

        let resp = aw_test::call_service(
            &app,
            aw_test::TestRequest::default()
                .method(Method::OPTIONS)
                .uri("/api/ping")
                .insert_header((header::ORIGIN, "https://evil.example.com"))
                .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "POST"))
                .to_request(),
        )
        .await;
        assert!(
            !resp.headers().contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "外站 origin 不能拿到 ACAO: {:?}",
            resp.headers()
        );
    }
}
