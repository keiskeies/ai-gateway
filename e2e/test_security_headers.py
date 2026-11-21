"""安全响应头与 CORS 的真实链路校验。

Rust 侧的单测（src/cors.rs、src/security_headers.rs）用手工搭的 `App` 跑，
只能证明"这段逻辑对"，证明不了 `src/main.rs` 真的把它 wrap 上了——
而"写了但没接"正是这类改动最常见的落地事故。这里打真起的实例，
测的就是装配本身。

顺带守住两个局域网约束（用户明确要求"不用太紧，我还是要局域网使用的"）：
`/wall` 允许同源套壳、以及回环 origin 必须放行。
"""

from __future__ import annotations

import urllib.error
import urllib.request


def _get(port: int, path: str, headers: dict[str, str] | None = None):
    """返回 (status, headers)。4xx/5xx 也照样取响应头，不当异常抛出。"""
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}", headers=headers or {}, method="GET"
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, response.headers
    except urllib.error.HTTPError as error:
        return error.code, error.headers


def test_console_responses_carry_security_headers(api):
    for path in ("/index.html", "/api/auth/status"):
        status, headers = _get(api.port, path)
        assert status == 200, path
        csp = headers.get("content-security-policy") or ""
        assert "frame-ancestors 'self'" in csp, f"{path} 缺 CSP: {csp!r}"
        assert "script-src 'self' 'unsafe-inline'" in csp, (
            f"{path} 的 script-src 会挡掉 index.html 的防闪烁主题脚本: {csp!r}"
        )
        assert headers.get("x-content-type-options") == "nosniff", path
        # XFO 只有 DENY/SAMEORIGIN 两档，一发就把"同源嵌 /wall"打成白屏。
        # 点击劫持由 frame-ancestors 覆盖，所以这条缺席是**要守的**。
        assert "x-frame-options" not in headers, f"{path} 不该带 X-Frame-Options"


def test_proxy_path_gets_no_extra_headers(api):
    """`/v1` 的响应是给 SDK 读的，多几行头只会出现在客户端日志与排障截图里。"""
    # /v1/models 是公开接口，不需要建 Key 也能拿到一份真响应
    status, headers = _get(api.port, "/v1/models")
    assert status == 200
    assert "content-security-policy" not in headers
    assert "x-content-type-options" not in headers


def test_cross_origin_session_read_is_denied(api):
    """这次改动的全部意义：跨源响应头不能再同时给出 ACAO 与 credentials。

    旧的 `Cors::permissive()` 会回 `Access-Control-Allow-Origin: *` 加
    `Access-Control-Allow-Credentials: true`。会话 cookie 是 `SameSite=Strict`，
    跨**站**本来就读不到，但同站不同端口（`localhost` 任意端口算同一站）Strict 不管，
    那时 permissive 就等于对本机任意进程开放 `/api`。这里只断言响应头这一层，
    与 cookie 属性互为纵深。
    """
    status, headers = _get(
        api.port, "/api/auth/status", {"Origin": "https://evil.example.com"}
    )
    assert status == 200, "同源语义不变：请求本身仍然成功，只是浏览器不给读"
    allow_origin = headers.get("access-control-allow-origin")
    assert allow_origin != "*", "不能再是通配符"
    assert allow_origin is None, f"外站 origin 不该被回显进 ACAO: {allow_origin!r}"
    assert headers.get("access-control-allow-credentials") is None, (
        "绝不能对跨源请求打开 credentials"
    )


def test_preflight_from_local_origin_is_allowed(api):
    """回环 origin 的预检必须过——桌面端端口发现与本机上别的开发服务器要用的就是这条路。

    用一次带 JSON 体的 POST 形态（而不是纯 GET）：`Cors::default()` 的方法表与请求头表
    都是空集，只配 origin 时预检全量 400，白名单写了等于没写。GET 反而测不出来，
    因为最简单的 GET 根本不发预检。
    """
    origin = f"http://127.0.0.1:{api.port}"
    request = urllib.request.Request(
        f"http://127.0.0.1:{api.port}/api/auth/login",
        method="OPTIONS",
        headers={
            "Origin": origin,
            "Access-Control-Request-Method": "POST",
            "Access-Control-Request-Headers": "content-type",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            headers, status = response.headers, response.status
    except urllib.error.HTTPError as error:
        headers, status = error.headers, error.code
    assert status in (200, 204), status
    assert headers.get("access-control-allow-origin") == origin
    assert "POST" in (headers.get("access-control-allow-methods") or "")
    assert headers.get("access-control-allow-credentials") is None, (
        "预检也不能打开 credentials"
    )


def test_preflight_from_lan_ip_origin_is_denied(api):
    """局域网 IP 的 origin 走的是跨源路径 —— 但它**不是**风险来源。

    这条记录的是当前决策而非"局域网不能用了"：从 `http://192.168.1.20:1994`
    打开控制台后，前端所有请求都是同源相对路径，浏览器根本不发预检，
    所以局域网使用完全不受影响。这里挡掉的只有"局域网里另一台机器上的
    某个网页"来跨源读 /api。若将来真需要那种用法，放宽 is_local_origin
    并同步改掉本用例，别默默放宽。
    """
    request = urllib.request.Request(
        f"http://127.0.0.1:{api.port}/api/auth/status",
        method="OPTIONS",
        headers={
            "Origin": "http://192.168.1.20:1994",
            "Access-Control-Request-Method": "GET",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            headers = response.headers
    except urllib.error.HTTPError as error:
        headers = error.headers
    assert headers.get("access-control-allow-origin") is None
