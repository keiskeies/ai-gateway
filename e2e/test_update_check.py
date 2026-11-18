"""检查更新：入口只在「桌面端 + 本机控制台」出现，其余场合整块不存在。

桌面端才有安装包落点与 reveal opener；服务器模式下后端两个写接口回 409，
前端连按钮都不渲染。这条规格同时锁住设置页像素基线（`/settings` 的无头浏览器
永远不是桌面端）。

`desktop_update` 只是进程能力位，对每个请求都一样为真；桌面端默认监听 0.0.0.0，
所以局域网那台机器也读得到它，但 `/api/update` 整段在 `require_local_console`
后面。入口必须同时看 `local_console`，否则画出来的是一整块永远 403 的按钮——
这条由 test_update_entry_absent_for_remote_console 守着。

桌面端那一半用路由桩跑：把 `desktop_update` 打开，检查 → 下载 → 进度 → 完成
的状态机就能在真浏览器里走一遍。桩要用 `re.compile` 而不是 `**/api/update**`
glob——glob 版漏掉了 `/api/update/download`，POST 会打到真后端拿 409。
"""

import json
import re

ASSET = "AI.Gateway_1.5.0_x64-setup.exe"

STUB_RELEASE = {
    "current": "1.4.8",
    "latest": "1.5.0",
    "has_update": True,
    "release_url": "https://github.com/keiskeies/ai-gateway/releases/tag/v1.5.0",
    "published_at": "2026-11-18T02:13:09Z",
    "assets": [
        {"name": "AI.Gateway_1.5.0_aarch64.dmg", "size": 12000000, "current_platform": False},
        {"name": ASSET, "size": 9000000, "current_platform": True},
    ],
}
# 又发了新版：手里那个 1.5.0 的包就此过时
STUB_RELEASE_NEWER = {
    **STUB_RELEASE,
    "latest": "1.6.0",
    "release_url": "https://github.com/keiskeies/ai-gateway/releases/tag/v1.6.0",
    "assets": [{"name": "AI.Gateway_1.6.0_x64-setup.exe", "size": 9100000, "current_platform": True}],
}

DONE_PATH = f"C:\\Users\\tester\\Downloads\\{ASSET}"


def _stub_status(route, *, local_console=True, desktop_update=True):
    route.fulfill(
        status=200,
        content_type="application/json",
        body=json.dumps(
            {
                "initialized": True,
                "authenticated": True,
                "username": "admin",
                "admin_port": 1994,
                "local_console": local_console,
                "checkin_login": False,
                "desktop_update": desktop_update,
            }
        ),
    )


def _progress(state, *, received=0, total=9000000, version="1.5.0", error=None):
    body = {"state": state, "name": ASSET, "received": received, "total": total, "version": version}
    if state == "done":
        body["path"] = DONE_PATH
    if error:
        body["error"] = error
    return body


def _buttons(page):
    return page.evaluate(
        """() => [...document.querySelectorAll('.console-update button')].map(el => el.textContent).join('|')"""
    )


def test_update_entry_absent_in_web_mode(console, page, api, errors):
    status, body = api.call("GET", "/api/auth/status")
    assert status == 200, body
    assert not body.get("desktop_update"), "独立服务器不该报告支持检查更新"

    console.goto(page, "/settings")
    page.wait_for_selector("#set-about", timeout=15000)
    assert page.locator(".console-update").count() == 0, "web 端设置页出现了检查更新入口"
    assert page.evaluate(
        """() => [...document.querySelectorAll('button')].some(el => el.textContent.includes('检查更新'))"""
    ) is False

    # 就算直接打接口也写不了盘：没注册 opener，下载与定位都是 409
    for method, path in (("POST", "/api/update/download"), ("POST", "/api/update/reveal")):
        status, body = api.call(method, path, {"asset": None})
        assert status == 409, f"{method} {path} 应回 409，实际 {status} {body}"

    assert errors == []


def test_update_entry_absent_for_remote_console(console, page, errors):
    """桌面端 + 远程浏览器：能力位为真，但下载落点是别人那台机器，整块不许渲染。"""
    page.route("**/api/auth/status", lambda route: _stub_status(route, local_console=False))

    console.goto(page, "/settings")
    page.wait_for_selector("#set-about", timeout=15000)
    assert page.locator(".console-update").count() == 0, (
        "远程控制台拿到了检查更新入口，而它每次点击都会撞 403"
    )
    assert errors == []


def test_update_flow_renders_and_downloads(console, page, errors):
    started = {"at": False}
    polls = {"n": 0}

    def stub_update(route):
        path = route.request.url.split("?")[0].rsplit("/api", 1)[-1]
        if path == "/update":
            route.fulfill(status=200, content_type="application/json", body=json.dumps(STUB_RELEASE))
        elif path == "/update/download":
            started["at"] = True
            route.fulfill(
                status=202, content_type="application/json", body=json.dumps(_progress("downloading"))
            )
        elif path == "/update/progress":
            if not started["at"]:
                body = _progress("idle")
            else:
                polls["n"] += 1
                # 先给两帧 downloading，进度条那一帧才真的能在屏上断言到
                body = (
                    _progress("downloading", received=3600000)
                    if polls["n"] <= 2
                    else _progress("done", received=9000000)
                )
            route.fulfill(status=200, content_type="application/json", body=json.dumps(body))
        elif path == "/update/reveal":
            route.fulfill(status=200, content_type="application/json", body='{"ok":true}')
        else:
            route.abort()

    page.route(re.compile(r"/api/update"), stub_update)
    page.route("**/api/auth/status", lambda route: _stub_status(route))

    console.goto(page, "/settings")
    page.wait_for_selector(".console-update", timeout=15000)

    console.click_text(page, "检查更新")
    page.wait_for_selector(".console-update-new", timeout=15000)
    latest = page.inner_text(".console-update-new")
    assert "1.5.0" in latest and "MB" in latest, f"新版本一行不对：{latest}"

    console.click_text(page, "下载 v1.5.0")
    page.wait_for_selector(".console-update-progress", timeout=15000)
    assert "%" in page.inner_text(".console-update-progress-text")
    # 下载中不给「打开所在文件夹」，也不给第二个下载按钮
    assert "打开所在文件夹" not in page.inner_text(".console-update")

    page.wait_for_selector(".console-update-done", timeout=15000)
    assert page.inner_text(".console-update-done-path") == DONE_PATH
    # 装的就是屏幕上这个版本：说的是「双击即可完成更新」，不是「先前下载的」
    assert "退出本程序后双击安装包" in page.inner_text(".console-update-hint")
    assert page.locator(".console-update-progress").count() == 0
    buttons = _buttons(page)
    # 下完之后能重新下（文件可能被删或被移动），也给了定位入口
    assert "打开所在文件夹" in buttons and "下载 v1.5.0" in buttons, buttons
    assert "查看发布说明" in buttons, buttons

    assert errors == []


def test_update_done_panel_names_the_version_it_holds(console, page, errors):
    """下了 1.5.0 没装，之后再查到 1.6.0：不能再说「双击即可完成更新」。"""
    checks = {"n": 0}

    def stub_update(route):
        path = route.request.url.split("?")[0].rsplit("/api", 1)[-1]
        if path == "/update":
            checks["n"] += 1
            release = STUB_RELEASE if checks["n"] == 1 else STUB_RELEASE_NEWER
            route.fulfill(status=200, content_type="application/json", body=json.dumps(release))
        elif path == "/update/download":
            route.fulfill(
                status=202,
                content_type="application/json",
                body=json.dumps(_progress("done", received=9000000)),
            )
        elif path == "/update/progress":
            body = _progress("done", received=9000000) if checks["n"] else _progress("idle")
            route.fulfill(status=200, content_type="application/json", body=json.dumps(body))
        else:
            route.abort()

    page.route(re.compile(r"/api/update"), stub_update)
    page.route("**/api/auth/status", lambda route: _stub_status(route))

    console.goto(page, "/settings")
    page.wait_for_selector(".console-update", timeout=15000)
    console.click_text(page, "检查更新")
    page.wait_for_selector(".console-update-new", timeout=15000)
    console.click_text(page, "下载 v1.5.0")

    console.click_text(page, "检查更新")
    page.wait_for_function(
        """() => document.querySelector('.console-update-new')?.textContent.includes('1.6.0')""",
        timeout=15000,
    )
    hint = page.inner_text(".console-update-hint")
    assert "v1.5.0" in hint and "不是当前列出的新版本" in hint, f"过时安装包没有说明：{hint}"
    # 文件确实还在下载目录里，所以定位入口要留着
    assert page.inner_text(".console-update-done-path") == DONE_PATH
    assert "打开所在文件夹" in _buttons(page)


def test_update_download_failure_stays_on_screen(console, page, errors):
    """下载失败只剩一条会自己关掉的 toast 等于没说：进度条已随 state 撤走，原因要留在面板上。"""
    started = {"at": False}
    polls = {"n": 0}

    def stub_update(route):
        path = route.request.url.split("?")[0].rsplit("/api", 1)[-1]
        if path == "/update":
            route.fulfill(status=200, content_type="application/json", body=json.dumps(STUB_RELEASE))
        elif path == "/update/download":
            started["at"] = True
            route.fulfill(
                status=202, content_type="application/json", body=json.dumps(_progress("downloading"))
            )
        elif path == "/update/progress":
            if not started["at"]:
                body = _progress("idle")
            else:
                polls["n"] += 1
                body = (
                    _progress("downloading", received=3600000)
                    if polls["n"] < 2
                    else _progress("error", error="下载中断: connection closed")
                )
            route.fulfill(status=200, content_type="application/json", body=json.dumps(body))
        else:
            route.abort()

    page.route(re.compile(r"/api/update"), stub_update)
    page.route("**/api/auth/status", lambda route: _stub_status(route))

    console.goto(page, "/settings")
    page.wait_for_selector(".console-update", timeout=15000)
    console.click_text(page, "检查更新")
    page.wait_for_selector(".console-update-new", timeout=15000)
    console.click_text(page, "下载 v1.5.0")
    page.wait_for_selector(".console-update-progress", timeout=15000)

    page.wait_for_selector(".console-update .console-note.warn", timeout=15000)
    note = page.inner_text(".console-update .console-note.warn")
    assert "下载没有完成" in note and "connection closed" in note, f"失败原因没落在面板上：{note}"
    # 进度条撤掉了，但下载按钮要能重来
    assert page.locator(".console-update-progress").count() == 0
    assert "下载 v1.5.0" in _buttons(page)

    assert errors == []
