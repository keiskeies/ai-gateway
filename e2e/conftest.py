"""控制台 E2E 夹具。

只打这里现起的隔离实例：动态端口、独立 config.toml、独立 data/、跑完整个目录删掉。
1994 上装好的那份和仓库根的 data/ 都不会被碰到——夹具会显式拒绝这两个端口。

需要的前置产物（夹具不会自己编译，只会报错并说明缺哪个）：
    cargo build                      → target/debug/ai-gateway.exe
    npm --prefix frontend run build  → static/
运行：
    python -m pytest e2e -q
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
EXE = REPO / "target" / "debug" / "ai-gateway.exe"
STATIC = REPO / "static"
AXE_JS = REPO / "frontend" / "node_modules" / "axe-core" / "axe.min.js"
RUNTIME_ROOT = Path(os.environ.get("AIGW_E2E_DIR", REPO / ".dsh-tmp" / "e2e"))
ADMIN_USER = "e2e-admin"
ADMIN_PASSWORD = "e2e-admin-password"

# 1994 是装好的生产实例，1995 是手工预览实例——夹具只用动态端口，撞上一个就停
FORBIDDEN_PORTS = {1994, 1995}


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def port_is_free(port: int) -> bool:
    with socket.socket() as probe:
        try:
            probe.bind(("127.0.0.1", port))
            return True
        except OSError:
            return False


CONFIG_TEMPLATE = """[server]
host = "127.0.0.1"
admin_port = {port}
log_level = "warn"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "e2e-only-key-not-for-production"
admin_token = ""

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
"""


class Api:
    """带会话 Cookie 的管理接口客户端。返回 (status, data)，JSON 自动解析。"""

    def __init__(self, port: int):
        self.port = port
        self.base = f"http://127.0.0.1:{port}"
        self.session: str | None = None

    def call(self, method: str, path: str, body=None, token: str | None = None, base: str | None = None):
        data = json.dumps(body).encode() if body is not None else None
        headers = {"Content-Type": "application/json"}
        if self.session:
            headers["Cookie"] = f"aigw_session={self.session}"
        if token:
            headers["Authorization"] = f"Bearer {token}"
        request = urllib.request.Request((base or self.base) + path, data=data, method=method, headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                self._capture_cookie(response.headers)
                raw = response.read().decode("utf-8", "replace")
                return response.status, self._parse(response.headers, raw)
        except urllib.error.HTTPError as error:
            self._capture_cookie(error.headers)
            raw = error.read().decode("utf-8", "replace")
            return error.code, self._parse(error.headers, raw)

    @staticmethod
    def _parse(headers, raw: str):
        if "json" in (headers.get("content-type") or ""):
            try:
                return json.loads(raw)
            except json.JSONDecodeError:
                return raw
        return raw

    def _capture_cookie(self, headers) -> None:
        cookie = headers.get("set-cookie") or ""
        if "aigw_session=" in cookie:
            self.session = cookie.split("aigw_session=", 1)[1].split(";", 1)[0]

    def setup_admin(self) -> None:
        status, body = self.call("POST", "/api/auth/setup", {"username": ADMIN_USER, "password": ADMIN_PASSWORD})
        assert status == 200, f"setup 失败 {status} {body}"
        assert self.session, "setup 后没拿到会话 Cookie"

    def login(self, username: str = ADMIN_USER, password: str = ADMIN_PASSWORD) -> None:
        status, body = self.call("POST", "/api/auth/login", {"username": username, "password": password})
        assert status == 200, f"登录失败 {status} {body}"
        assert self.session, "登录后没拿到会话 Cookie"

    def wait_stats(self, seconds: float = 1.2) -> None:
        """统计走 StatsWriter 的 500ms 批量落库，响应回来时那一行通常还没进表。"""
        time.sleep(seconds)


class Instance:
    def __init__(self, name: str, port: int | None = None):
        # 端口写死是视觉回归要的前提：端口出现在侧栏页脚、设置页的管理端口和数据库路径里，
        # 每次换端口就是在改屏上文字，像素 diff 会变成随机失败
        port = port if port is not None and port_is_free(port) else free_port()
        if port in FORBIDDEN_PORTS:
            raise RuntimeError(f"动态端口撞上 {port}（生产/预览实例），拒绝启动")
        self.name = name
        self.port = port
        self.dir = RUNTIME_ROOT / f"{name}-{port}"
        self.dir.mkdir(parents=True, exist_ok=True)
        if not EXE.exists():
            pytest.fail(f"缺少后端产物 {EXE}——先跑 cargo build")
        if not (STATIC / "index.html").exists():
            pytest.fail(f"缺少前端产物 {STATIC}/index.html——先跑 npm --prefix frontend run build")
        shutil.copy2(EXE, self.dir / EXE.name)
        if (self.dir / "static").exists():
            shutil.rmtree(self.dir / "static")
        shutil.copytree(STATIC, self.dir / "static")
        (self.dir / "config.toml").write_text(CONFIG_TEMPLATE.format(port=port), encoding="utf-8")
        log = (self.dir / "server.log").open("ab")
        self.process = subprocess.Popen([str(self.dir / EXE.name)], cwd=str(self.dir), stdout=log, stderr=log)
        self.api = Api(port)
        self._wait_health()

    def _wait_health(self) -> None:
        deadline = time.time() + 40
        while time.time() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"实例 {self.name} 启动即退出（看 {self.dir}/server.log）")
            try:
                with urllib.request.urlopen(f"{self.api.base}/health", timeout=2) as response:
                    if response.status == 200:
                        return
            except (urllib.error.URLError, OSError):
                time.sleep(0.3)
        raise RuntimeError(f"实例 {self.name} 40s 内没有 /health")

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
        # SQLite 句柄可能还在释放，删不干净就重试几次
        deadline = time.time() + 8
        while self.dir.exists() and time.time() < deadline:
            try:
                shutil.rmtree(self.dir)
            except OSError:
                time.sleep(0.4)


@pytest.fixture(scope="session")
def instances():
    """工厂夹具：一个 session 里可以起多个实例（首次向导要一份全新库，不能被别的用例污染）。"""
    created: list[Instance] = []

    def make(name: str, port: int | None = None) -> Instance:
        instance = Instance(name, port)
        created.append(instance)
        return instance

    yield make
    for instance in created:
        instance.stop()


@pytest.fixture(scope="session")
def api(instances) -> Api:
    instance = instances("main")
    instance.api.setup_admin()
    return instance.api


@pytest.fixture(scope="session")
def mock_upstream():
    """假上游：只为让链路里出现一次真实 200，不接任何外部服务、不花真钱。

    正文必须带 created——它是必填 i64，缺了 openai::parse_response 会失败，
    而解析失败的那条路径在记统计之前就透传返回了（明细行根本不会存在）。
    """

    state = {"hits": 0}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args) -> None:
            pass

        def _read_body(self) -> str:
            length = int(self.headers.get("content-length") or 0)
            return self.rfile.read(length).decode("utf-8", "replace") if length else ""

        def do_GET(self) -> None:  # noqa: N802
            if self.path.endswith("/models"):
                self._json({"object": "list", "data": [{"id": "mock-mini", "object": "model"}]})
            else:
                self._json({"error": "not found"}, status=404)

        def do_POST(self) -> None:  # noqa: N802
            raw = self._read_body()
            if not self.path.endswith("/chat/completions"):
                self._json({"error": "not found"}, status=404)
                return
            state["hits"] += 1
            text = f"mock answer #{state['hits']}"
            try:
                streaming = bool(json.loads(raw).get("stream"))
            except json.JSONDecodeError:
                streaming = False
            if streaming:
                self.send_response(200)
                self.send_header("content-type", "text/event-stream")
                self.end_headers()
                for chunk in (
                    {"id": "mock", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {"content": text}}]},
                    {"id": "mock", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
                ):
                    self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
                self.wfile.write(b"data: [DONE]\n\n")
                return
            self._json(
                {
                    "id": "mock",
                    "object": "chat.completion",
                    "created": int(time.time()),
                    "model": "mock-mini",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 12, "completion_tokens": 5, "total_tokens": 17},
                }
            )

        def _json(self, payload, status: int = 200) -> None:
            body = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", free_port()), Handler)
    runner = threading.Thread(target=server.serve_forever, daemon=True)
    runner.start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}/v1"
    finally:
        server.shutdown()
        server.server_close()


class Console:
    """浏览器侧的公共动作。

    这套控制台在 headless Playwright 下真实鼠标点击不会到达 React，
    所以点击一律走 element.click()（等价于键盘激活），输入走原生 setter + input 事件。
    """

    def __init__(self, context, base: str):
        self.context = context
        self.base = base
        self._seq = 0

    def new_page(self):
        return self.context.new_page()

    @staticmethod
    def select_option(page, container: str, query: str) -> None:
        """Mantine Select：合成点击打不开下拉，focus + 方向键可以。

        可搜索的那个会跟着键入过滤；不可搜索的输入框是只读的，键入是空操作，
        所以两种都走"focus → 键入 → 方向键 → 按文本点选项"。
        """
        selector = f"{container} .mantine-Select-input"
        page.wait_for_selector(selector, timeout=10000)
        page.evaluate("(sel) => document.querySelector(sel).focus()", selector)
        page.keyboard.type(query, delay=25)
        page.keyboard.press("ArrowDown")
        page.wait_for_selector("[role=option]", timeout=5000)
        page.evaluate(
            """(q) => {
                const nodes = [...document.querySelectorAll('[role=option]')];
                const target = nodes.find(el => el.textContent.trim() === q) ?? nodes.find(el => el.textContent.includes(q));
                if (!target) throw new Error(`下拉里没有 ${q}：${nodes.map(n => n.textContent.trim()).join(' | ')}`);
                target.click();
            }""",
            query,
        )

    def goto(self, page, route: str = "/", mount: str = ".console-shell"):
        self._seq += 1
        hash_part = route if route.startswith("#") else "/" + route.lstrip("/")
        # hash 路由下只改 hash 是同文档跳转，不重新加载文档；query 换值才是真的整页加载
        page.goto(f"{self.base}/?v={self._seq}#{hash_part}", wait_until="domcontentloaded")
        # 挂载信号按外壳给：`/wall` 是 .wall-root，没有 .console-shell
        page.wait_for_selector(mount, timeout=15000)
        self._install_toasts(page)
        return page

    @staticmethod
    def _install_toasts(page) -> None:
        page.evaluate(
            """() => {
                window.__toasts = [];
                new MutationObserver(records => {
                    for (const record of records) {
                        for (const node of record.addedNodes) {
                            if (node.nodeType !== 1) continue;
                            const text = (node.textContent || '').trim();
                            if (text) window.__toasts.push(text);
                        }
                    }
                }).observe(document.body, { childList: true, subtree: true });
            }"""
        )

    def toasts(self, page) -> list[str]:
        page.wait_for_timeout(400)
        return page.evaluate("() => { const t = window.__toasts || []; window.__toasts = []; return t }")

    @staticmethod
    def click(page, selector: str) -> None:
        page.wait_for_selector(selector, timeout=10000)
        page.evaluate(
            """(sel) => document.querySelectorAll(sel)[document.querySelectorAll(sel).length - 1].click()""",
            selector,
        )

    @staticmethod
    def click_text(page, text: str, selector: str = "button") -> None:
        """按可见文案点按钮。取最后一个匹配——弹窗挂在 body 末尾，所以弹窗里的同名按钮是最后一个。"""
        page.wait_for_function(
            """([sel, text]) => [...document.querySelectorAll(sel)].some(el => el.textContent.trim() === text)""",
            arg=[selector, text],
            timeout=10000,
        )
        page.evaluate(
            """([sel, text]) => {
                const nodes = [...document.querySelectorAll(sel)].filter(el => el.textContent.trim() === text);
                nodes[nodes.length - 1].click();
            }""",
            [selector, text],
        )

    @staticmethod
    def set_input(page, selector: str, value: str) -> None:
        page.wait_for_selector(selector, timeout=10000)
        page.evaluate(
            """([sel, value]) => {
                const el = document.querySelector(sel);
                const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
                Object.getOwnPropertyDescriptor(proto.prototype, 'value').set.call(el, value);
                el.dispatchEvent(new Event('input', { bubbles: true }));
            }""",
            [selector, value],
        )

    @staticmethod
    def set_field_by_label(page, container: str, value: str, *labels: str) -> None:
        """按字段 label 找输入框（label 是大写 kicker，中英文各试一遍），比 placeholder 稳。"""
        page.wait_for_selector(container, timeout=10000)
        page.evaluate(
            """([container, labels, value]) => {
                const wanted = labels.map(l => l.trim().toLowerCase());
                const field = [...document.querySelectorAll(`${container} .console-field`)].find(el => {
                    const label = el.querySelector('label');
                    return label && wanted.includes(label.textContent.trim().toLowerCase());
                });
                if (!field) throw new Error(`找不到字段 ${labels[0]}（${container}）`);
                const el = field.querySelector('textarea, input');
                const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
                Object.getOwnPropertyDescriptor(proto.prototype, 'value').set.call(el, value);
                el.dispatchEvent(new Event('input', { bubbles: true }));
            }""",
            [container, list(labels), value],
        )

    @staticmethod
    def text(page, selector: str) -> str:
        page.wait_for_selector(selector, timeout=10000)
        return page.evaluate("(sel) => document.querySelector(sel).textContent.trim()", selector)

    @staticmethod
    def audit(page, tags: tuple[str, ...] = ("wcag2a", "wcag2aa", "wcag21a", "wcag21aa")) -> list[dict]:
        """注入仓库里的 axe-core 跑机器可判定的 WCAG 规则——不联网、不装扩展。

        返回按规则聚合的违规（规则 id / impact / 前几个选择器 / 命中节点数），
        这样断言失败时读得懂，不必再跑一遍去猜。
        """
        if not AXE_JS.exists():
            pytest.fail(f"缺少 axe-core（{AXE_JS}）——先跑 npm --prefix frontend install")
        page.add_script_tag(path=str(AXE_JS))
        page.wait_for_function("() => window.axe !== undefined", timeout=10000)
        result = page.evaluate(
            """(tags) => axe.run(document, {
                runOnly: { type: 'tag', values: tags },
                resultTypes: ['violations'],
            })""",
            list(tags),
        )
        return [
            {
                "id": v["id"],
                "impact": v["impact"],
                "help": v["help"],
                "count": len(v["nodes"]),
                "nodes": [
                    {
                        "target": n["target"][0],
                        "html": (n["html"] or "")[:160],
                        "why": (n.get("failureSummary") or "")[:220],
                    }
                    for n in v["nodes"][:3]
                ],
            }
            for v in result["violations"]
        ]


@pytest.fixture(scope="session")
def browser_factory():
    """一个 session 只起一次 Playwright；需要独立实例的用例（首次向导、整库还原）各自要一份 context。"""
    from playwright.sync_api import sync_playwright

    with sync_playwright() as playwright:
        launched = []

        def make(api: Api, init: str = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')") -> Console:
            browser = playwright.chromium.launch()
            launched.append(browser)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            context.add_init_script(init)
            if api.session:
                context.add_cookies([{"name": "aigw_session", "value": api.session, "url": api.base}])
            return Console(context, api.base)

        yield make
        for browser in launched:
            browser.close()


@pytest.fixture(scope="session")
def console(browser_factory, api) -> Console:
    return browser_factory(api)


@pytest.fixture
def page(console):
    handle = console.new_page()
    yield handle
    handle.close()


@pytest.fixture
def errors(page):
    """收集 console error / pageerror，用例结尾断言为空——UI 回归最常见的回归就是这个。"""
    collected: list[str] = []
    page.on("console", lambda message: collected.append(message.text) if message.type == "error" else None)
    page.on("pageerror", lambda error: collected.append(str(error)))
    yield collected


@pytest.fixture
def mock_stack(api, mock_upstream):
    """假上游平台 → 同步模型 → 虚拟模型 + 路由 → 网关 Key，用完按子→父删掉。

    路由必须先有 models 行（backends.model_id 有外键），所以走一次 sync；
    POST /api/proxies 只建空壳（routes: []），路由与首个后端是一次 POST /routes。
    """
    tag = f"e2e{int(time.time() * 1000) % 100000:05d}"
    status, platform = api.call(
        "POST", "/api/platforms", {"name": tag, "type": "OpenAI", "base_url": mock_upstream, "api_key": "mock-key"}
    )
    assert status == 201, f"建平台失败 {status} {platform}"
    status, synced = api.call("POST", f"/api/platforms/{platform['id']}/models/sync")
    assert status == 200 and synced.get("created") == 1, f"同步模型失败 {status} {synced}"
    status, proxy = api.call("POST", "/api/proxies", {"name": tag})
    assert status == 201, f"建虚拟模型失败 {status} {proxy}"
    status, route = api.call(
        "POST", f"/api/proxies/{proxy['id']}/routes", {"backends": [{"platform_id": platform["id"], "model_id": "mock-mini"}]}
    )
    assert status == 201, f"建路由失败 {status} {route}"
    secret = f"sk-{tag}-fixed-key-1234567890"
    status, key = api.call("POST", "/api/api-keys", {"name": tag, "key": secret})
    assert status == 201, f"建 Key 失败 {status} {key}"
    yield {
        "tag": tag,
        "platform": platform,
        "proxy": proxy,
        "route": route,
        "key": key,
        "secret": secret,
        # 网关按虚拟模型名解析路由（RouteCache 的键是 proxy name），不是后端 model_id
        "model": proxy["name"],
        "backend_model": "mock-mini",
    }
    api.call("DELETE", f"/api/api-keys/{key['id']}")
    api.call("DELETE", f"/api/proxies/{proxy['id']}")
    api.call("DELETE", f"/api/platforms/{platform['id']}")


def gateway_chat(api: Api, stack: dict, prompt: str, stream: bool = False):
    return api.call(
        "POST",
        "/v1/chat/completions",
        {"model": stack["model"], "messages": [{"role": "user", "content": prompt}], "stream": stream},
        token=stack["secret"],
    )
