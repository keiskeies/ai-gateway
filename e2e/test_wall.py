"""§11.8 监控大屏（/wall）+ §12.7 的 1280/1440/1920 × 深浅走查。

/wall 不在 AppShell 里：它自己就是一屏，没有侧栏与顶栏，退出靠右下角那颗固定按钮，
所以别的页面钉不住它，要单独审两件事：
① 数据真的上屏（7 张 KPI 卡都有读数、矩阵里有那一笔请求的模型行、热力格在）；
② 三个宽度 × 两套配色下不横向溢出、读数不被裁、响应式断点真的按 CSS 的媒体查询换列数。
断点期望用页面里的 matchMedia 反推，不自己写死宽度边界——1280 正好压在
`.wall-pair` 的 max-width: 1279px 上，视口宽度算不算滚动条会决定这一格走哪个分支。
截图落在 .dsh-tmp/wall/ 供人眼复核；像素级视觉回归仍未自动化。
"""

import json
from datetime import datetime
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import pytest

REPO = Path(__file__).resolve().parents[1]
VIEWPORTS = [(1280, 800), (1440, 900), (1920, 1080)]
SHOTS = REPO / ".dsh-tmp" / "wall"

INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"

# 读数被裁的判定：声明了 text-overflow: ellipsis 的（矩阵单元格）是有意截断，
# 没声明却被 overflow: hidden 切掉的（KPI 大数字）是事故。
LAYOUT_PROBE = """() => {
    const problems = [];
    const vw = window.innerWidth, vh = window.innerHeight;
    const de = document.documentElement;
    if (de.scrollWidth > vw + 1) problems.push(`整页横向溢出 scrollWidth=${de.scrollWidth} innerWidth=${vw}`);

    const clipped = (el) => {
        if (getComputedStyle(el).textOverflow === 'ellipsis') return false;
        return el.scrollWidth > el.clientWidth + 1;
    };
    for (const sel of ['.console-readout-value', '.console-readout-label', '.console-readout-foot',
                       '.console-page-title', '.wall-panel-title', '.wall-panel-sub', '.wall-rank-label'])
        for (const el of document.querySelectorAll(sel))
            if (clipped(el))
                problems.push(`${sel} 读数被裁 scrollWidth=${el.scrollWidth} clientWidth=${el.clientWidth} "${el.textContent.trim().slice(0, 24)}"`);

    for (const sel of ['.wall-page-head', '.wall-readouts', '.wall-pair',
                       '.wall-panel', '.wall-heatmap', '.wall-heat-hours'])
        for (const el of document.querySelectorAll(sel)) {
            const r = el.getBoundingClientRect();
            if (r.right > vw + 1 || r.left < -1)
                problems.push(`${sel} 越出视口 left=${Math.round(r.left)} right=${Math.round(r.right)} vw=${vw}`);
        }

    const tracks = (sel) => getComputedStyle(document.querySelector(sel)).gridTemplateColumns.split(' ').length;
    const expectedKpi = window.matchMedia('(max-width: 767px)').matches ? 2
        : window.matchMedia('(max-width: 1279px)').matches ? 4 : 7;
    if (tracks('.wall-readouts') !== expectedKpi)
        problems.push(`.wall-readouts 列数 ${tracks('.wall-readouts')}，期望 ${expectedKpi}`);
    const stacked = window.matchMedia('(max-width: 1279px)').matches ? 1 : 2;
    if (tracks('.wall-pair') !== stacked) problems.push(`.wall-pair 列数 ${tracks('.wall-pair')}，期望 ${stacked}`);

    const cell = document.querySelector('.wall-heat-cell');
    if (!cell) problems.push('没有热力格');
    else if (cell.getBoundingClientRect().width < 6) problems.push(`热力格被压到 ${cell.getBoundingClientRect().width.toFixed(1)}px`);

    // Y 轴刻度不许重复：请求数只有 1 时 recharts 默认按小数分档、再被 tickFormatter 取整，
    // 屏上就会出现 "1 / 1 / 1 / 0 / 0" 这种读不出量级的轴。
    const ticks = [...document.querySelectorAll('.recharts-yAxis .recharts-cartesian-axis-tick-value')]
        .map(el => el.textContent.trim());
    if (ticks.length && new Set(ticks).size !== ticks.length) problems.push(`Y 轴刻度重复 [${ticks.join(' / ')}]`);

    const exit = document.querySelector('.wall-route-exit');
    if (!exit) problems.push('没有退出大屏按钮');
    else {
        const r = exit.getBoundingClientRect();
        if (r.bottom > vh + 1 || r.right > vw + 1) problems.push(`退出按钮越出视口 ${Math.round(r.right)}×${Math.round(r.bottom)}`);
    }
    return problems;
}"""


def one_request(api, stack) -> None:
    status, _ = api.call(
        "POST",
        "/v1/chat/completions",
        {"model": stack["model"], "messages": [{"role": "user", "content": f"大屏走查-{stack['tag']}"}]},
        token=stack["secret"],
    )
    assert status == 200, f"网关请求失败 {status}"
    api.wait_stats()


def test_wall_shows_data_and_exits(console, page, api, errors, mock_stack):
    one_request(api, mock_stack)
    console.goto(page, "/wall", ".wall-root")
    # 矩阵按后端 model_id 聚合（网关名只出现在健康看板与明细的"网关"列），所以等的是 mock-mini
    page.wait_for_function(
        """(name) => [...document.querySelectorAll('.wall-mx-model')].some(el => el.textContent.trim() === name)""",
        arg=mock_stack["backend_model"],
        timeout=15000,
    )

    # 大屏外壳：没有 AppShell 的侧栏与顶栏，页底由 .wall-route 自己铺
    assert page.evaluate("() => document.documentElement.classList.contains('wall-route')")
    assert page.evaluate("() => document.querySelectorAll('.mantine-AppShell-navbar').length") == 0

    kpis = page.evaluate(
        """() => [...document.querySelectorAll('.wall-readouts .console-readout')].map(k => ({
            label: k.querySelector('.console-readout-label')?.textContent.trim(),
            value: k.querySelector('.console-readout-value')?.textContent.trim(),
        }))"""
    )
    assert len(kpis) == 7, f"KPI 卡 {len(kpis)} 张：{kpis}"
    assert all(k["value"] for k in kpis), f"有 KPI 卡没有读数：{kpis}"
    assert page.locator(".wall-heat-cell").count() == 7 * 24

    # 那一笔请求要在矩阵里，且请求数不是 0——否则上屏的是骨架或聚合口径错了
    # 那一笔请求要在矩阵里。计数是累计的（同一 session 里别的用例也打的是 mock-mini），
    # 所以断"至少 1 且是数字"，不断等于 1。
    row = page.evaluate(
        """(name) => {
            const td = [...document.querySelectorAll('.wall-mx-model')].find(el => el.textContent.trim() === name);
            if (!td) return null;
            return [...td.parentElement.querySelectorAll('td')].map(el => el.textContent.trim());
        }""",
        mock_stack["backend_model"],
    )
    assert row and row[1].isdigit() and int(row[1]) >= 1, f"矩阵行不对：{row}"

    # 退出：客户端路由回 /，AppShell 接管
    console.click(page, ".wall-route-exit")
    page.wait_for_selector(".console-shell", timeout=10000)
    assert page.evaluate("() => document.documentElement.classList.contains('wall-route')") is False
    assert errors == []


@pytest.mark.parametrize("scheme", ["dark", "light"])
def test_wall_names_what_failed_and_recovers(api, mock_stack, browser_factory, scheme):
    """§12.7 无静默失败：掐掉两条统计接口，屏上要点名是哪几块取不到，重试取到了就收掉。

    只掐 heatmap 与 errors：其余模块照常出数，才能看出点名点的是谁、没点名的确实没被牵连。
    """
    console = browser_factory(api, init=f"{INIT}; localStorage.setItem('theme', '{scheme}')")
    page = console.new_page()
    # 掐接口必然留下 net::ERR_FAILED 的资源日志，所以只听 pageerror，不听 console
    thrown: list[str] = []
    page.on("pageerror", lambda error: thrown.append(str(error)))
    one_request(api, mock_stack)
    for pattern in ("**/api/stats/heatmap*", "**/api/stats/errors*"):
        page.route(pattern, lambda route: route.abort())
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-failbar", timeout=15000)

    bar = page.inner_text(".wall-failbar")
    assert "活跃热力图" in bar and "错误明细" in bar, f"告警条没点名：{bar!r}"
    # 取到了的模块不能被牵连进来，否则"点名"退化成"整页都坏了"
    for ok in ("模型性能矩阵", "平台分布", "虚拟模型健康看板"):
        assert ok not in bar, f"取到了的模块被误报：{bar!r}"

    # 局部失败不该塌成整页：KPI 与矩阵还在
    assert page.locator(".wall-readouts .console-readout").count() == 7
    assert page.locator(".wall-mx-model").count() >= 1

    # 面板不能退回"暂无数据"：那是把"没取到"说成"确实没有"
    panels = {
        p["title"]: p["text"]
        for p in page.evaluate(
            """() => [...document.querySelectorAll('.wall-panel')].map(p => ({
                title: p.querySelector('.wall-panel-title')?.textContent.trim(),
                text: p.innerText.replace(/\\s+/g, ' '),
            }))"""
        )
    }
    assert "暂无统计数据" not in panels.get("活跃热力图", ""), f"热力图仍谎称没数据：{panels.get('活跃热力图')!r}"
    assert "取不到" in panels.get("活跃热力图", ""), f"热力图没标失败态：{panels.get('活跃热力图')!r}"
    
    # KPI 成功率卡的副行来自错误统计：取不到时是 —，不是"成功 0 · 失败 0"
    kpis = page.evaluate("""() => [...document.querySelectorAll('.wall-readouts .console-readout')].map(k => k.innerText.replace(/\\s+/g, ' '))""")
    assert any("成功 — · 失败 —" in k for k in kpis), f"成功率副行给了假 0：{kpis}"
    SHOTS.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(SHOTS / f"wall-failbar-{scheme}.png"))

    page.unroute("**/api/stats/heatmap*")
    page.unroute("**/api/stats/errors*")
    console.click(page, ".wall-failbar button")
    page.wait_for_selector(".wall-failbar", state="detached", timeout=15000)
    assert page.locator(".wall-heat-cell").count() == 7 * 24
    # 重试取到了就退回真读数，不留 —
    dashes = page.evaluate("""() => [...document.querySelectorAll('.wall-readouts .console-readout-value')].filter(el => el.textContent.trim() === '—').length""")
    assert dashes == 0, f"重试后仍有 {dashes} 个 — 读数"
    assert "成功 —" not in page.inner_text(".wall-readouts")

    page.close()
    assert thrown == [], f"取数失败不该让页面抛错：{thrown}"


def test_logs_table_expands_without_inner_scroll(console, page, api, mock_stack):
    """请求明细不设内层滚动：整表撑开，滚动交给页面。"""
    one_request(api, mock_stack)
    rows = [
        {
            "id": i,
            "created_at": "2026-10-22T10:00:00Z",
            "model_id": "mock-mini",
            "proxy_name": "gw",
            "platform_name": "upstream",
            "status_code": 200,
            "token_input": 12,
            "fresh_input_tokens": 12,
            "token_output": 5,
            "cache_read_tokens": 0,
            "cache_write_tokens": 0,
            "cost": None,
            "latency_ms": 4,
            "ttft_ms": None,
            "output_speed_tps": 1250.0,
            "is_stream": False,
            "error_type": None,
        }
        for i in range(60)
    ]
    page.route(
        "**/api/stats/requests*",
        lambda route: route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps({"total": 120, "page": 1, "page_size": 50, "items": rows}),
        ),
    )
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-logs tbody tr", timeout=15000)
    metrics = page.evaluate(
        """() => {
            const el = document.querySelector('.wall-logs-scroll');
            return {
                rows: document.querySelectorAll('.wall-logs tbody tr').length,
                scrollHeight: el.scrollHeight,
                clientHeight: el.clientHeight,
                pageHeight: document.documentElement.scrollHeight,
            };
        }"""
    )
    assert metrics["rows"] == 60, f"明细只有 {metrics['rows']} 行"
    assert metrics["scrollHeight"] <= metrics["clientHeight"] + 1, f"明细表仍有内层纵向滚动：{metrics}"
    assert metrics["pageHeight"] > 2000, f"页面没有随明细表撑开：{metrics}"


def test_heatmap_has_its_own_range(console, page, api, mock_stack):
    """热力图不跟页头走：默认 7 天，切到 30 天要重新取数且窗口更长。"""
    one_request(api, mock_stack)
    seen: list[str] = []

    def capture(route):
        seen.append(route.request.url)
        route.continue_()

    page.route("**/api/stats/heatmap*", capture)
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-heatmap", timeout=15000)

    for _ in range(20):
        if seen:
            break
        page.wait_for_timeout(100)
    assert seen, "热力图没有发起请求"

    # 热力图面板自己的 30 天选项（页头也有 30 天，必须限定在面板内）
    page.evaluate(
        """() => {
            const panel = [...document.querySelectorAll('.wall-pair .wall-panel')]
                .find(p => p.querySelector('.wall-heatmap'));
            const input = panel.querySelector('.mantine-SegmentedControl-root input[value="30d"]');
            if (!input) throw new Error('热力图面板里没有 30 天选项');
            input.click();
        }"""
    )
    for _ in range(30):
        if len(seen) >= 2:
            break
        page.wait_for_timeout(100)
    assert len(seen) >= 2, f"切换时段后没有重新请求：{seen}"

    def span_days(url: str) -> float:
        q = parse_qs(urlparse(url).query)
        start = datetime.fromisoformat(q["from"][0].replace("Z", "+00:00"))
        end = datetime.fromisoformat(q["to"][0].replace("Z", "+00:00"))
        return (end - start).total_seconds() / 86400

    first, second = span_days(seen[0]), span_days(seen[-1])
    assert 6.5 < first < 7.5, f"默认窗口不是 7 天：{first:.2f}"
    assert second > 29, f"切到 30 天后窗口仍只有 {second:.2f} 天"


def test_matrix_failure_drilldown(console, page, api, mock_stack):
    """失败数点开后就在该行下方展开错误类型明细，不再单独占一块面板。"""
    one_request(api, mock_stack)

    def summaries(route):
        resp = route.fetch()
        data = resp.json()
        for row in data:
            row["failed_requests"] = 3
            row["requests"] = max(int(row.get("requests") or 0), 4)
        route.fulfill(status=200, content_type="application/json", body=json.dumps(data))

    seen: list[str] = []

    def errors(route):
        seen.append(route.request.url)
        scoped = "model=" in route.request.url
        body = (
            {"total": 3, "success": 0, "errors": [{"kind": "RateLimit", "count": 3}]}
            if scoped
            else {"total": 0, "success": 0, "errors": []}
        )
        route.fulfill(status=200, content_type="application/json", body=json.dumps(body))

    page.route("**/api/stats/model-summaries*", summaries)
    page.route("**/api/stats/errors*", errors)
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_selector(".wall-mx-failed", timeout=15000)

    console.click(page, ".wall-mx-failed")
    page.wait_for_selector(".wall-mx-err-row", timeout=10000)
    text = page.inner_text(".wall-mx-err-row")
    assert "3" in text, f"明细里没有失败数：{text!r}"
    SHOTS.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(SHOTS / "wall-drilldown.png"), full_page=True)
    assert any("model=" in url for url in seen), f"下钻请求没带 model：{seen}"

    # 再点一次收起，明细行消失
    console.click(page, ".wall-mx-failed")
    page.wait_for_selector(".wall-mx-err-row", state="detached", timeout=10000)


@pytest.mark.parametrize("scheme", ["dark", "light"])
def test_wall_layout_at_each_viewport(api, mock_stack, browser_factory, scheme):
    """三个宽度 × 当前配色：溢出、裁字、断点三件事一次过，并留一张截图。"""
    console = browser_factory(api, init=f"{INIT}; localStorage.setItem('theme', '{scheme}')")
    page = console.new_page()
    one_request(api, mock_stack)

    problems: list[str] = []
    seen: list[str] = []
    for width, height in VIEWPORTS:
        page.set_viewport_size({"width": width, "height": height})
        console.goto(page, "/wall", ".wall-root")
        page.wait_for_function(
            """(name) => [...document.querySelectorAll('.wall-mx-model')].some(el => el.textContent.trim() === name)""",
            arg=mock_stack["backend_model"],
            timeout=15000,
        )
        # 趋势图/环形是 recharts，晚一帧才落位；等它再量，否则量到的是半张图
        page.wait_for_selector(".wall-matrix-scroll table", timeout=10000)
        page.wait_for_timeout(600)
        found = page.evaluate(LAYOUT_PROBE)
        problems += [f"{width}x{height} {scheme}：{p}" for p in found]
        shape = page.evaluate(
            """() => ({
                pageHeight: document.documentElement.scrollHeight,
                kpiColumns: getComputedStyle(document.querySelector('.wall-readouts')).gridTemplateColumns.split(' ').length,
                mainColumns: getComputedStyle(document.querySelector('.wall-pair')).gridTemplateColumns.split(' ').length,
            })"""
        )
        seen.append(f"{width}x{height} {scheme} → {shape}")
        SHOTS.mkdir(parents=True, exist_ok=True)
        page.screenshot(path=str(SHOTS / f"wall-{scheme}-{width}x{height}.png"), full_page=True)

    page.close()
    print("\n".join(seen))
    assert problems == [], "\n".join(problems)
