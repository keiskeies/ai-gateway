use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::error::InternalError;
use actix_web::middleware::Next;
use actix_web::http::header::HOST;
use actix_web::{HttpRequest, ResponseError};

use crate::error::AppError;

/// 本机控制台判定：三个条件同时成立。
/// 1. TCP peer 是回环地址（远程直连一定不是）；
/// 2. 没有 `Forwarded` / `X-Forwarded-*` / `X-Real-IP`（反向代理会带）；
/// 3. Host 是 `127.0.0.1` / `localhost` / `::1`（nginx 通常会把 Host 改成外部域名）。
///
/// 边界：如果反向代理把 Host 保留成 127.0.0.1 且不写任何转发头，这里无法区分；
/// 这种部署必须让代理设置 `X-Forwarded-For`/`X-Real-IP`，否则客户端接入会被当成
/// 本机控制台。SSH 端口转发到 127.0.0.1 属于本机访问，允许。
pub fn is_local_console(req: &HttpRequest) -> bool {
    let peer_ok = req
        .peer_addr()
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false);
    if !peer_ok {
        return false;
    }
    let forwarded = req.headers().keys().any(|name| {
        let name = name.as_str().to_ascii_lowercase();
        name == "forwarded" || name == "x-real-ip" || name.starts_with("x-forwarded-")
    });
    if forwarded {
        return false;
    }
    let Some(host) = req.headers().get(HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    host_is_loopback(host)
}

fn host_is_loopback(raw: &str) -> bool {
    let host = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else if host.matches(':').count() == 1 {
        // host:port；IPv6 无方括号时有多个冒号，不按端口切
        host.split(':').next().unwrap_or(host.as_str())
    } else {
        host.as_str()
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// `/api/clients/*` 的入口守卫：不是本机控制台一律 403，即使知道完整 URL 也改不了本地文件。
pub async fn require_local_console<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<B>, actix_web::Error>
where
    B: MessageBody,
{
    if !is_local_console(req.request()) {
        return Err(InternalError::from_response(
            "",
            AppError::Forbidden(
                "Client access is only available from the local console".to_string(),
            )
            .error_response(),
        )
        .into());
    }
    next.call(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn request(peer: &str, host: Option<&str>, forwarded: Option<(&str, &str)>) -> HttpRequest {
        let mut req = TestRequest::default().peer_addr(peer.parse().unwrap());
        if let Some(host) = host {
            req = req.insert_header(("Host", host));
        }
        if let Some((name, value)) = forwarded {
            req = req.insert_header((name, value));
        }
        req.to_http_request()
    }

    #[test]
    fn loopback_peer_and_host_is_local() {
        assert!(is_local_console(&request("127.0.0.1:5000", Some("127.0.0.1:1994"), None)));
        assert!(is_local_console(&request("127.0.0.1:5000", Some("localhost:1994"), None)));
        assert!(is_local_console(&request("127.0.0.1:5000", Some("[::1]:1994"), None)));
        assert!(is_local_console(&request("127.0.0.1:5000", Some("127.0.0.1"), None)));
    }

    #[test]
    fn remote_host_peer_or_proxy_is_not_local() {
        // 远程直连：peer 不是回环
        assert!(!is_local_console(&request("192.168.1.50:5000", Some("127.0.0.1:1994"), None)));
        // nginx 改 Host 成外部域名
        assert!(!is_local_console(&request("127.0.0.1:5000", Some("gateway.example.com"), None)));
        // 反向代理转发头
        assert!(!is_local_console(&request("127.0.0.1:5000", Some("127.0.0.1:1994"), Some(("X-Forwarded-For", "1.2.3.4")))));
        assert!(!is_local_console(&request("127.0.0.1:5000", Some("127.0.0.1:1994"), Some(("Forwarded", "for=1.2.3.4")))));
        // 缺 Host 不猜成本机
        assert!(!is_local_console(&request("127.0.0.1:5000", None, None)));
    }
}
