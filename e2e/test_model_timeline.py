"""单模型分时性能：模式切换、矩阵下钻、空桶/未定价语义、共享读数与可访问性。

数据直接写进隔离实例的 SQLite（时间点可控），不依赖实时请求的桶位置。
"""

import sqlite3
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
SHOTS = REPO / ".dsh-tmp" / "timeline"
INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"


def _seed_instance(instance, upstream: str) -> None:
    api = instance.api
    api.setup_admin()

    status, platform = api.call(
        "POST", "/api/platforms",
        {"name": "tl-platform", "type": "OpenAI", "base_url": upstream, "api_key": "tl-key"},
    )
    assert status == 201, f"建平台失败 {status} {platform}"
    status, synced = api.call("POST", f"/api/platforms/{platform['id']}/models/sync")
    assert status == 200 and synced.get("created") == 1, f"同步模型失败 {status} {synced}"

    status, proxy = api.call("POST", "/api/proxies", {"name": "tl-single"})
    assert status == 201, f"建虚拟模型失败 {status} {proxy}"

    db_path = instance.dir / "data" / "ai-gateway.db"
    with sqlite3.connect(db_path, timeout=15) as conn:
        # 第二个上游模型：直接插 models 行，路由只校验 (platform_id, model_id) 是否存在
        conn.execute(
            "INSERT INTO models (id, platform_id, model_id, display_name, max_tokens, context_window, input_price, output_price, capabilities, status) "
            "VALUES ('tl-large-model', ?, 'mock-large', 'mock-large', 4096, 8192, NULL, NULL, '[]', 'Active')",
            (platform["id"],),
        )

    status, route = api.call(
        "POST", f"/api/proxies/{proxy['id']}/routes",
        {"backends": [
            {"platform_id": platform["id"], "model_id": "mock-mini"},
            {"platform_id": platform["id"], "model_id": "mock-large"},
        ]},
    )
    assert status == 201, f"建路由失败 {status} {route}"

    with sqlite3.connect(db_path, timeout=15) as conn:
        backends = {model_id: backend_id for backend_id, model_id in conn.execute("SELECT id, model_id FROM backends")}
        assert set(backends) == {"mock-mini", "mock-large"}, f"后端不对：{backends}"
        proxy_id = proxy["id"]
        route_id = route["id"]

        # 以整点附近为锚：-3h 两笔（一笔流式已定价、一笔非流式未定价），-1h 失败一笔；
        # -2h 留空，最后一桶（当前小时）也留空——用来验证 0 与 null 的区别。
        now = datetime.now(timezone.utc).replace(minute=15, second=0, microsecond=0)
        t3 = (now - timedelta(hours=3)).isoformat()
        t1 = (now - timedelta(hours=1)).isoformat()
        rows = [
            (proxy_id, route_id, backends["mock-mini"], 200, 100, 1000, 400, None, t3, 100, 600, 100, 1, None, 0.01, None, None),
            (proxy_id, route_id, backends["mock-mini"], 200, 300, 500, 100, None, t3, None, None, None, 0, None, None, None, None),
            (proxy_id, route_id, backends["mock-mini"], 500, 5000, None, None, "ServerError", t1, None, None, None, 0, None, None, None, None),
            (proxy_id, route_id, backends["mock-large"], 200, 200, 300, 50, None, t1, None, None, None, 0, None, None, None, None),
        ]
        conn.executemany(
            "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, token_input, token_output, "
            "error_type, created_at, ttft_ms, cache_read_tokens, cache_write_tokens, is_stream, api_key_id, cost, request_id, replayed_from) "
            "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            rows,
        )
        conn.commit()


@pytest.fixture(scope="module")
def timeline_instance(instances, mock_upstream):
    instance = instances("timeline")
    _seed_instance(instance, mock_upstream)
    return instance


def _open_single(console, page, api) -> None:
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-mx-model-btn", timeout=15000)
    # 页头时段：默认「今日」在凌晨可能不覆盖 -3h，固定切到 24h
    page.evaluate(
        """() => {
            const input = document.querySelector('.wall-page-head .mantine-SegmentedControl-root input[value="24h"]');
            if (!input) throw new Error('页头没有 24h 选项');
            input.click();
        }"""
    )
    page.wait_for_selector('.wall-trend-panel .mantine-SegmentedControl-root input[value="single"]', state="attached", timeout=15000)
    page.evaluate(
        """() => {
            const input = document.querySelector('.wall-trend-panel .mantine-SegmentedControl-root input[value="single"]');
            input.click();
        }"""
    )
    page.wait_for_selector(".wall-timeline", timeout=15000)
    page.wait_for_timeout(500)


def _select_timeline_model(page, model: str) -> None:
    """时间线模型 Select 与请求明细的模型筛选同时在 DOM 里，必须只点可见下拉选项。"""
    selector = ".wall-trend-panel .mantine-Select-input"
    page.wait_for_selector(selector, timeout=10000)
    page.evaluate("(sel) => document.querySelector(sel).focus()", selector)
    page.keyboard.press("Control+A")
    page.keyboard.type(model, delay=25)
    page.keyboard.press("ArrowDown")
    page.wait_for_function(
        """(model) => [...document.querySelectorAll('[role=option]')].some(el => {
            const r = el.getBoundingClientRect();
            return r.width > 0 && r.height > 0 && el.textContent.trim() === model;
        })""",
        arg=model,
        timeout=5000,
    )
    page.evaluate(
        """(model) => {
            const target = [...document.querySelectorAll('[role=option]')].find(el => {
                const r = el.getBoundingClientRect();
                return r.width > 0 && r.height > 0 && el.textContent.trim() === model;
            });
            if (!target) throw new Error(`下拉里没有 ${model}`);
            target.click();
        }""",
        model,
    )
    page.wait_for_function(
        "(model) => document.querySelector('.wall-trend-panel .mantine-Select-input')?.value === model",
        arg=model,
        timeout=5000,
    )
    page.keyboard.press("Escape")


def _readout_chips(page) -> dict[str, str]:
    return page.evaluate(
        """() => Object.fromEntries(
            [...document.querySelectorAll('.wall-timeline-chip')].map(chip => [
                chip.querySelector('.wall-timeline-chip-label').textContent.trim(),
                chip.querySelector('.wall-timeline-chip-value').textContent.trim(),
            ])
        )"""
    )


def test_single_model_view_buckets_gaps_and_model_switch(timeline_instance, browser_factory):
    api = timeline_instance.api
    console = browser_factory(api, init=INIT)
    page = console.new_page()
    errors: list[str] = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)

    _open_single(console, page, api)

    # 默认选请求最多的模型；5 条 lane 都在（成本有已定价请求）
    assert page.input_value(".wall-trend-panel .mantine-Select-input") == "mock-mini"
    lane_titles = page.evaluate(
        "() => [...document.querySelectorAll('.wall-timeline-lane-title')].map(el => el.textContent.trim())"
    )
    joined = " | ".join(lane_titles)
    for expected in ("流量与失败", "延迟与首字", "输出速度", "Token 构成", "成本估算"):
        assert expected in joined, f"缺少 lane {expected}：{joined}"

    # 窗口汇总来自 model-summaries 同一行：3 请求 / 1 失败 / 1500 输入 / 800 新增 / 500 输出 / 600 缓存 / 0.01 成本
    chips = _readout_chips(page)
    assert chips["请求"] == "3", chips
    assert chips["失败"].startswith("1"), chips
    assert chips["输入"] == "1.5K", chips
    assert chips["新增输入"] == "800", chips
    assert chips["输出"] == "500", chips
    assert chips["缓存"] == "600", chips
    assert chips["成本"] == "0.01", chips
    assert chips["延时"] == "1.80s", chips
    assert "最后一桶尚未结束" in page.inner_text(".wall-timeline-notes")
    assert "UTC" in page.inner_text(".wall-timeline-notes")
    assert "没有请求记录" in page.inner_text(".wall-timeline-notes")
    cost_title = page.evaluate(
        """() => [...document.querySelectorAll('.wall-timeline-chip')]
            .find(chip => chip.querySelector('.wall-timeline-chip-label').textContent.trim() === '成本')?.title || ''"""
    )
    assert "33%" in cost_title, cost_title
    SHOTS.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(SHOTS / "single-model-dark.png"), full_page=True)

    # sr-only 表：0 请求桶的均值列必须是 —，不是 0
    rows = page.evaluate(
        """() => [...document.querySelectorAll('.wall-timeline table.console-sr-only tbody tr')]
            .map(tr => [...tr.querySelectorAll('td')].map(td => td.textContent.trim()))"""
    )
    zero_rows = [row for row in rows if row[1] == "0"]
    assert zero_rows, "没有找到 0 请求桶"
    assert all(row[8] == "—" and row[9] == "—" and row[10] == "—" for row in zero_rows), zero_rows[:3]
    assert all(row[11] == "—" for row in zero_rows), zero_rows[:3]
    assert sum(int(row[1].replace(",", "")) for row in rows) == 3, "桶请求数之和必须等于汇总"

    # 悬停切到单桶读数：标题从「本时段汇总」变成具体时段
    box = page.locator(".wall-timeline-lane-chart").first.bounding_box()
    assert box, "找不到第一条 lane"
    page.mouse.move(box["x"] + box["width"] * 0.6, box["y"] + box["height"] / 2)
    page.wait_for_timeout(300)
    title = page.evaluate("() => document.querySelector('.wall-timeline-readout-title')?.textContent ?? null")
    assert title is not None, (errors, page.evaluate("() => document.querySelector('.wall-timeline')?.outerHTML?.slice(0, 1200)"))
    assert title != "本时段汇总", title

    # 切到未定价且无缓存的模型：成本 lane 消失，读数成本为 —，notes 说明原因
    _select_timeline_model(page, "mock-large")
    page.wait_for_timeout(500)
    assert page.input_value(".wall-trend-panel .mantine-Select-input") == "mock-large"
    titles = page.inner_text(".wall-timeline")
    assert "成本估算" not in page.evaluate(
        "() => [...document.querySelectorAll('.wall-timeline-lane-title')].map(el => el.textContent).join(' | ')"
    )
    chips = _readout_chips(page)
    assert chips["请求"] == "1", chips
    assert chips["成本"] == "—", chips
    assert chips["首字"] == "—", chips
    assert "该模型未定价" in page.inner_text(".wall-timeline-notes")
    assert "本期无缓存命中" in titles
    assert "本期无流式请求" in titles
    assert "本期 0 失败" in titles

    # 矩阵按钮是下钻入口：点 mock-mini 应切回单模型视图
    page.evaluate(
        """() => {
            const button = [...document.querySelectorAll('.wall-mx-model-btn')].find(el => el.textContent.includes('mock-mini'));
            if (!button) throw new Error('矩阵里没有 mock-mini 按钮');
            button.click();
        }"""
    )
    page.wait_for_timeout(500)
    assert page.input_value(".wall-trend-panel .mantine-Select-input") == "mock-mini"

    # 模式切换是 radio、模型选择是 input：键盘焦点必须真的能移动
    page.evaluate("() => document.querySelector('.wall-trend-panel input[value=\"single\"]').focus()")
    before = page.evaluate("() => document.activeElement?.tagName")
    page.keyboard.press("Tab")
    after = page.evaluate("() => ({ tag: document.activeElement?.tagName, cls: String(document.activeElement?.className || '') })")
    assert before in ("INPUT", "BUTTON"), before
    assert after["tag"] in ("INPUT", "BUTTON"), after
    assert "mantine-Select-input" in after["cls"], after

    violations = console.audit(page)
    assert not violations, violations

    assert errors == [], errors
    page.close()


def test_single_model_failure_is_named_not_zero(timeline_instance, browser_factory):
    api = timeline_instance.api
    console = browser_factory(api, init=INIT)
    page = console.new_page()
    page.errors: list[str] = []
    page.on("pageerror", lambda error: page.errors.append(str(error)))

    page.route("**/api/stats/model-timeseries*", lambda route: route.abort())
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-mx-model-btn", timeout=15000)
    page.evaluate(
        """() => {
            const button = [...document.querySelectorAll('.wall-mx-model-btn')].find(el => el.textContent.includes('mock-mini'));
            if (!button) throw new Error('矩阵里没有 mock-mini 按钮');
            button.click();
        }"""
    )
    page.wait_for_selector(".wall-failbar", timeout=15000)

    assert "模型请求统计" in page.inner_text(".wall-failbar")
    assert page.locator(".wall-timeline").count() == 0, "取数失败时不该画一条 0 线"
    assert "这一块的数取不到" in page.inner_text(".wall-trend-panel")
    assert page.errors == [], page.errors
    page.close()


VIEWPORTS = [(1280, 800), (1440, 900), (1920, 1080)]
SINGLE_PROBE = """() => {
    const problems = [];
    const vw = window.innerWidth;
    const de = document.documentElement;
    if (de.scrollWidth > vw + 1) problems.push(`整页横向溢出 ${de.scrollWidth} > ${vw}`);
    const charts = [...document.querySelectorAll('.wall-timeline-lane-chart')];
    if (charts.length < 4) problems.push(`lane 数不足 ${charts.length}`);
    for (const chart of charts) {
        const r = chart.getBoundingClientRect();
        if (r.width < 200 || r.height < 60) problems.push(`lane 被压扁 ${r.width.toFixed(0)}×${r.height.toFixed(0)}`);
        if (r.left < -1 || r.right > vw + 1) problems.push(`lane 越出视口 ${Math.round(r.left)}-${Math.round(r.right)}`);
    }
    const panel = document.querySelector('.wall-trend-panel');
    if (!panel) problems.push('没有统计面板');
    else {
        const r = panel.getBoundingClientRect();
        if (r.left < -1 || r.right > vw + 1) problems.push(`面板越出视口 ${Math.round(r.left)}-${Math.round(r.right)}`);
    }
    const chips = document.querySelectorAll('.wall-timeline-chip');
    if (chips.length < 10) problems.push(`读数 chip 只有 ${chips.length} 个`);
    const select = document.querySelector('.wall-trend-panel .mantine-Select-input');
    if (!select) problems.push('没有模型选择器');
    else {
        const r = select.getBoundingClientRect();
        if (r.width < 80 || r.right > vw + 1) problems.push(`模型选择器被压扁/越界 ${r.width.toFixed(0)}`);
    }
    if (!document.querySelector('.wall-timeline-readout')) problems.push('没有读数条');
    if (!document.querySelector('.wall-route-exit')) problems.push('没有退出大屏按钮');
    return problems;
}"""


def test_single_model_layout_at_each_viewport_light(timeline_instance, browser_factory):
    """单模型视图在三个宽度 + 亮色下不溢出、lane 不塌，且 axe 0 违规。"""
    api = timeline_instance.api
    console = browser_factory(api, init=f"{INIT}; localStorage.setItem('theme', 'light')")
    page = console.new_page()
    problems: list[str] = []
    for width, height in VIEWPORTS:
        page.set_viewport_size({"width": width, "height": height})
        _open_single(console, page, api)
        page.wait_for_timeout(600)
        found = page.evaluate(SINGLE_PROBE)
        problems += [f"{width}x{height} light：{item}" for item in found]
        SHOTS.mkdir(parents=True, exist_ok=True)
        page.screenshot(path=str(SHOTS / f"single-model-light-{width}x{height}.png"), full_page=True)

    # 全屏样式给趋势面板写死了 400px；单模型模式必须覆盖成 auto，否则最后几条 lane 会被裁掉
    fullscreen = page.evaluate(
        """() => {
            document.documentElement.classList.add('wall-fullscreen');
            const panel = document.querySelector('.wall-trend-panel');
            const lanes = [...document.querySelectorAll('.wall-timeline-lane-chart')];
            const panelRect = panel.getBoundingClientRect();
            const last = lanes[lanes.length - 1]?.getBoundingClientRect();
            const result = {
                panelHeight: panelRect.height,
                laneHeight: lanes[0]?.getBoundingClientRect().height || 0,
                lastLaneInside: Boolean(last && last.bottom <= panelRect.bottom + 1),
            };
            document.documentElement.classList.remove('wall-fullscreen');
            return result;
        }"""
    )
    assert fullscreen["laneHeight"] > 90, fullscreen
    assert fullscreen["lastLaneInside"], fullscreen

    violations = console.audit(page)
    assert not violations, violations
    assert problems == [], "\n".join(problems)
    page.close()
