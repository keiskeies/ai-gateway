"""大屏成本展示：KPI / 矩阵 / 趋势 / 平台占比 / 预算告警。

直接往隔离实例的 SQLite 里塞已定价与未定价的请求，价格和高峰窗口也写库，
所以断言的是"给定价格与 token 后大屏应该显示什么"，不依赖上游 mock 的 usage。

覆盖的语义：
- 未定价模型在矩阵里写「未定价」，不是 0；
- 成本 KPI 带覆盖率（已定价 1 / 共 2 = 50%），并显示当前高峰模型数；
- 缓存价差按请求时冻结的金额展示（token 卡脚注）；
- 平台分布可切到成本口径，中心数字与图例都是金额；
- Key 配额告警由大屏自己触发 `/api/alerts` 的读时扫描，并出现在预算告警条。
"""

import sqlite3
from datetime import datetime, timezone

import pytest

INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"

# 全天覆盖的高峰窗口：测试跑在任何时刻都处于高峰，不依赖本机时钟
PEAK_ALL_DAY = (
    '{"windows":[{"start":"00:00","end":"12:00"},{"start":"12:00","end":"00:00"}],'
    '"peak":{"input":3.0,"output":9.0,"cache_read":0.6}}'
)


@pytest.fixture(scope="module")
def cost_instance(instances, mock_upstream):
    instance = instances("wallcost")
    api = instance.api
    api.setup_admin()

    tag = "wall-cost"
    status, platform = api.call(
        "POST", "/api/platforms",
        {"name": tag, "type": "OpenAI", "base_url": mock_upstream, "api_key": "mock-key"},
    )
    assert status == 201, f"建平台失败 {status} {platform}"
    status, synced = api.call("POST", f"/api/platforms/{platform['id']}/models/sync")
    assert status == 200 and (synced.get("created") == 1 or synced.get("existing") == 1), \
        f"同步模型失败 {status} {synced}"

    db_path = instance.dir / "data" / "ai-gateway.db"
    with sqlite3.connect(db_path, timeout=15) as conn:
        catalog = conn.execute(
            "SELECT catalog_key FROM platforms WHERE id = ?", (platform["id"],)
        ).fetchone()[0]
        # 第二个上游模型：未定价，用来验证矩阵里的「未定价」和 50% 覆盖率
        conn.execute(
            "INSERT INTO models (id, platform_id, catalog_key, model_id, display_name, max_tokens, "
            "context_window, input_price, output_price, capabilities, status) "
            "VALUES ('wc-large', ?, ?, 'mock-large', 'mock-large', 4096, 8192, NULL, NULL, '[]', 'Active')",
            (platform["id"], catalog),
        )
        # mock-mini：基础价 + 缓存命中价 + 全天高峰覆盖
        conn.execute(
            "UPDATE models SET input_price = 2.0, output_price = 8.0, cache_read_price = 0.5, "
            "peak_pricing = ? WHERE platform_id = ? AND model_id = 'mock-mini'",
            (PEAK_ALL_DAY, platform["id"]),
        )

    status, proxy = api.call("POST", "/api/proxies", {"name": tag})
    assert status == 201, f"建虚拟模型失败 {status} {proxy}"
    status, route = api.call(
        "POST", f"/api/proxies/{proxy['id']}/routes",
        {"backends": [
            {"platform_id": platform["id"], "model_id": "mock-mini"},
            {"platform_id": platform["id"], "model_id": "mock-large"},
        ]},
    )
    assert status == 201, f"建路由失败 {status} {route}"

    secret = "sk-wall-cost-budget-1234567890"
    status, key = api.call(
        "POST", "/api/api-keys",
        {"name": "wall-budget", "key": secret, "quota_requests": 1},
    )
    assert status == 201, f"建 Key 失败 {status} {key}"

    with sqlite3.connect(db_path, timeout=15) as conn:
        backends = dict(conn.execute("SELECT model_id, id FROM backends").fetchall())
        now = datetime.now(timezone.utc).isoformat()
        # 已定价：fresh 800 × 2 + hit 200 × 0.5 + out 500 × 8，全部 /1e6 = 0.0057
        # 缓存价差：200 × (2 − 0.5) / 1e6 = 0.0003
        conn.execute(
            "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, "
            "token_input, token_output, error_type, created_at, ttft_ms, cache_read_tokens, "
            "cache_write_tokens, is_stream, api_key_id, cost, cache_savings, request_id, replayed_from) "
            "VALUES (?,?,?,200,100,1000,500,NULL,?,100,200,0,1,?,0.0057,0.0003,NULL,NULL)",
            (proxy["id"], route["id"], backends["mock-mini"], now, key["id"]),
        )
        # 未定价：同一平台的第二个模型
        conn.execute(
            "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, "
            "token_input, token_output, error_type, created_at, ttft_ms, cache_read_tokens, "
            "cache_write_tokens, is_stream, api_key_id, cost, cache_savings, request_id, replayed_from) "
            "VALUES (?,?,?,200,200,300,50,NULL,?,NULL,NULL,NULL,0,NULL,NULL,NULL,NULL,NULL)",
            (proxy["id"], route["id"], backends["mock-large"], now),
        )
        conn.commit()

    return instance


def _open_wall(cost_instance, browser_factory):
    console = browser_factory(cost_instance.api, init=INIT)
    page = console.new_page()
    console.goto(page, "/wall", ".wall-root")
    page.wait_for_function(
        """() => [...document.querySelectorAll('.wall-mx-model')].some(el => el.textContent.trim() === 'mock-large')""",
        timeout=15000,
    )
    return console, page


def _card(page, label: str) -> dict:
    return page.evaluate(
        """(label) => {
            const card = [...document.querySelectorAll('.wall-readouts .console-readout')]
                .find((el) => el.querySelector('.console-readout-label')?.textContent.trim() === label);
            if (!card) return null;
            return {
                value: card.querySelector('.console-readout-value')?.textContent.trim(),
                foot: card.querySelector('.console-readout-foot')?.innerText.replace(/\\s+/g, ' ').trim(),
            };
        }""",
        label,
    )


def test_cost_kpi_and_matrix_mark_unpriced(cost_instance, browser_factory):
    console, page = _open_wall(cost_instance, browser_factory)

    cost = _card(page, "成本估算")
    assert cost is not None, "没有成本 KPI 卡"
    assert cost["value"] == "0.0057", f"成本读数不对：{cost}"
    assert "覆盖 50%" in cost["foot"], f"成本脚注没有覆盖率：{cost}"
    assert "高峰 1/1" in cost["foot"], f"成本脚注没有高峰状态：{cost}"

    tokens = _card(page, "总 Token")
    assert "缓存节省 0.0003" in tokens["foot"], f"缓存价差没上屏：{tokens}"

    rows = page.evaluate(
        """() => {
            const out = {};
            for (const tr of document.querySelectorAll('.wall-matrix tbody tr')) {
                const name = tr.querySelector('.wall-mx-model-name')?.textContent.trim();
                if (name) out[name] = [...tr.children].map((td) => td.textContent.trim());
            }
            return out;
        }"""
    )
    # 列顺序：模型 0 · 请求 1 · 失败 2 · 成功率 3 · 入 4 · fresh 5 · 出 6 · 成本 7
    assert rows["mock-mini"][7] == "0.0057", f"已定价模型的成本列不对：{rows['mock-mini']}"
    assert rows["mock-large"][7] == "未定价", f"未定价模型必须写未定价，不是 0：{rows['mock-large']}"

    # 平台分布切到成本口径：图例写金额，中心也是金额
    page.evaluate(
        """() => {
            const panel = document.querySelectorAll('.wall-pair .wall-panel')[1];
            const input = [...panel.querySelectorAll('.mantine-SegmentedControl-root input')]
                .find((el) => el.value === 'cost');
            if (!input) throw new Error('平台分布没有成本口径');
            input.click();
        }"""
    )
    page.wait_for_function(
        """() => {
            const panel = document.querySelectorAll('.wall-pair .wall-panel')[1];
            return panel && panel.innerText.includes('覆盖 50%') && panel.innerText.includes('0.0057');
        }""",
        timeout=10000,
    )
    panel_text = page.inner_text(".wall-pair .wall-panel:nth-child(2)")
    assert "0.0057" in panel_text and "100%" in panel_text, f"成本分布没显示金额：{panel_text!r}"

    # 预算告警：Key 配额 1 条已用满，由大屏的 /api/alerts 扫描生成
    page.wait_for_selector(".wall-budgetbar", timeout=10000)
    budget = page.inner_text(".wall-budgetbar")
    assert "预算告警" in budget and "配额已用满" in budget, f"预算告警条不对：{budget!r}"

    page.close()


def test_cost_metric_in_trend_chart(cost_instance, browser_factory):
    console, page = _open_wall(cost_instance, browser_factory)

    page.evaluate(
        """() => {
            const input = [...document.querySelectorAll('.wall-trend-panel .mantine-SegmentedControl-root input')]
                .find((el) => el.value === 'total_cost');
            if (!input) throw new Error('趋势图没有成本指标');
            input.click();
        }"""
    )
    page.wait_for_function(
        """() => {
            const panel = document.querySelector('.wall-trend-panel');
            return panel && panel.querySelector('.wall-panel-sub')?.textContent.includes('覆盖 50%');
        }""",
        timeout=10000,
    )
    # 成本序列真的画出来了（未定价桶留空，已定价桶有一条线）
    assert page.locator(".wall-trend-panel .recharts-line").count() >= 1, "成本趋势没有折线"

    page.close()
