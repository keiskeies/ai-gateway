"""每日领取接口：平台即账号。

登录窗口只有 Tauri 桌面端才有，E2E 起的是独立服务，所以这里钉的是：
- 没有桌面壳时登录接口明确 409，能力表如实标记 login_supported=false；
- 会话绑在 platform_id 上，删平台要级联删掉会话；
- 假密文领取落成账号状态 error，不是接口 500；run-all 不因单个账号失败而整体报错。
"""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

INSERT_ACCOUNT = """INSERT INTO checkin_accounts
   (id, platform_id, platform, identity, label, cookies_enc, cookie_names, status,
    last_message, last_run_at, last_run_day, last_ok_day, balance, balance_at, daily_amount,
    daily_done, created_at, updated_at)
   VALUES ('e2e-acc', ?, 'modelscope', 'e2e-user', 'ModelScope 魔搭 · e2e-user',
           'fake-cipher', 'm_session_id,csn', 'ok', '今日已到账 100 魔粒（每日规则 1/1）',
           '2026-10-31T08:18:00Z', '2026-10-31', '2026-10-31', 1828,
           '2026-10-31T08:18:00Z', 100, 1,
           '2026-10-31T07:00:00Z', '2026-10-31T08:18:00Z')"""


def test_checkin_is_platform_bound_and_cascades(instances):
    instance = instances("checkin")
    api = instance.api
    api.setup_admin()

    status, body = api.call("GET", "/api/checkin/accounts")
    assert status == 200, body
    modelscope = next(item for item in body["platforms"] if item["key"] == "modelscope")
    assert modelscope["login_supported"] is False, "独立服务器模式没有登录窗口"
    assert modelscope["base_url_prefix"] == "https://api-inference.modelscope.cn"
    assert body["accounts"] == []

    # 新平台流程与已有平台登录都必须明确说「仅桌面端」
    status, body = api.call("POST", "/api/checkin/login", {"platform": "modelscope"})
    assert status == 409, body
    assert "桌面" in body["error"]["message"]

    status, platform = api.call(
        "POST",
        "/api/platforms",
        {
            "name": "ModelScope 魔搭",
            "type": "OpenAI",
            "base_url": "https://api-inference.modelscope.cn/v1",
            "api_key": "sk-e2e-modelscope-key",
            "credit_budget": 250,
            "credit_window": "day",
            "credit_unit": "credits",
        },
    )
    assert status == 201, platform
    platform_id = platform["id"]

    status, body = api.call("POST", f"/api/checkin/platforms/{platform_id}/login")
    assert status == 409, body
    assert "桌面" in body["error"]["message"]

    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(INSERT_ACCOUNT, (platform_id,))
    db.commit()
    db.close()

    status, body = api.call("GET", "/api/checkin/accounts")
    account = body["accounts"][0]
    assert account["platform_id"] == platform_id
    assert account["identity"] == "e2e-user"
    assert account["balance"] == 1828 and account["daily_amount"] == 100
    # 接口只回元信息，密文不许出现在响应里
    assert "cookies_enc" not in account and "fake-cipher" not in str(body)

    # 平台页积分列用领取同步的余额；本地台账（spent）不受影响
    status, credits = api.call("GET", "/api/platforms/credits")
    assert status == 200, credits
    credit = next(item for item in credits if item["platform_id"] == platform_id)
    assert credit["live_balance"] == 1828
    assert credit["balance_at"].startswith("2026-10-31T08:18:00")
    assert credit["spent"] == 0
    # 还没领取过 → 总额仍是配置预算（余额只进 tooltip）
    assert credit["budget_effective"] == 250

    # 手动领取要解密 cookie：假密文落成 error（账号状态），不是接口 500
    status, body = api.call("POST", f"/api/checkin/platforms/{platform_id}/run")
    assert status == 200, body
    assert body["status"] == "error"
    assert "登录态" in body["message"]

    # 失败的一次领取不能把上次同步到的余额/时间冲掉
    status, credits = api.call("GET", "/api/platforms/credits")
    credit = next(item for item in credits if item["platform_id"] == platform_id)
    assert credit["live_balance"] == 1828
    assert credit["balance_at"].startswith("2026-10-31T08:18:00")

    # 一键领取：单个账号失败只体现在账号状态里
    status, body = api.call("POST", "/api/checkin/run-all")
    assert status == 200, body
    assert len(body["accounts"]) == 1 and body["accounts"][0]["status"] == "error"

    # 退出登录只丢会话，平台保留
    status, _ = api.call("DELETE", f"/api/checkin/platforms/{platform_id}/session")
    assert status == 204
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["accounts"] == []

    # 删平台要级联删掉会话
    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(INSERT_ACCOUNT, (platform_id,))
    db.commit()
    db.close()
    status, _ = api.call("DELETE", f"/api/platforms/{platform_id}")
    assert status == 204
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["accounts"] == []


def test_credit_ledger_follows_the_claimed_balance_not_the_budget(instances):
    """领取同步的余额是本窗口总额：低于预算要显示低，高于预算不能夹回预算。

    曾经按 `spent = 预算 - 余额` 换算，余额一旦超过预算就被夹成 0，
    列上于是永远显示配置的 250——用户看到的就是「每次签到都变成 250」。
    """
    instance = instances("credit-anchor")
    api = instance.api
    api.setup_admin()

    status, platform = api.call(
        "POST",
        "/api/platforms",
        {
            "name": "ModelScope 魔搭",
            "type": "OpenAI",
            "base_url": "https://api-inference.modelscope.cn/v1",
            "api_key": "sk-e2e-modelscope-key",
            "credit_budget": 250,
            "credit_window": "day",
            "credit_unit": "credits",
        },
    )
    assert status == 201, platform
    platform_id = platform["id"]

    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(INSERT_ACCOUNT, (platform_id,))
    db.commit()
    db.close()

    # 直接改账号余额模拟领取读到 366，再走一次真实校准入口需要上游接口，
    # 所以这里只钉读数层的口径：锚点存在时总额取锚点。
    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(
        "INSERT OR REPLACE INTO platform_credit_state (platform_id, period_start, spent, anchor, updated_at) "
        "VALUES (?, ?, 0, 366, ?)",
        (platform_id, _period_start("day"), "2026-10-31T08:18:00Z"),
    )
    db.commit()
    db.close()

    status, credits = api.call("GET", "/api/platforms/credits")
    assert status == 200, credits
    credit = next(item for item in credits if item["platform_id"] == platform_id)
    assert credit["budget_effective"] == 366, "总额要跟着站点余额走"
    assert credit["remaining"] == 366
    assert credit["budget"] == 250, "配置的那条预算原样保留"


def _period_start(window: str) -> str:
    """与后端一致的自然窗口起点（本地零点换算成 UTC）。"""
    from datetime import datetime, timedelta, timezone

    now = datetime.now().astimezone()
    today = now.date()
    if window == "week":
        today = today - timedelta(days=today.weekday())
    elif window == "month":
        today = today.replace(day=1)
    local_midnight = datetime.combine(today, datetime.min.time()).astimezone()
    return local_midnight.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S+00:00")


def test_checkin_tasks_are_validated_runnable_and_pruned(instances):
    """定时领取任务：校验、立即运行、停用、删平台后剔除引用。

    登录窗口在 E2E 里不存在，所以任务只钉「领取」这一条链路；会话用假密文，
    「立即运行」必然解密失败 → 汇总里如实写「1 领取失败」，且不会发出真实请求。
    """
    instance = instances("checkin-task")
    api = instance.api
    api.setup_admin()

    status, platform = api.call(
        "POST",
        "/api/platforms",
        {
            "name": "ModelScope 魔搭",
            "type": "OpenAI",
            "base_url": "https://api-inference.modelscope.cn/v1",
            "api_key": "sk-e2e-modelscope-key",
        },
    )
    assert status == 201, platform
    platform_id = platform["id"]

    status, other = api.call(
        "POST",
        "/api/platforms",
        {
            "name": "DeepSeek",
            "type": "OpenAI",
            "base_url": "https://api.deepseek.com/v1",
            "api_key": "sk-e2e-deepseek-key",
        },
    )
    assert status == 201, other

    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["tasks"] == [], "新库不该有任务"

    # 时间不合法 / 空清单 / 不支持登录的平台，都要 400 并且说清为什么
    status, body = api.call(
        "POST", "/api/checkin/tasks", {"name": "", "time": "25:00", "platform_ids": [platform_id]}
    )
    assert status == 400, body
    assert "HH:MM" in body["error"]["message"]

    status, body = api.call(
        "POST", "/api/checkin/tasks", {"name": "", "time": "00:05", "platform_ids": []}
    )
    assert status == 400, body
    assert "至少选择一个账号" in body["error"]["message"]

    status, body = api.call(
        "POST", "/api/checkin/tasks", {"name": "", "time": "00:05", "platform_ids": [other["id"]]}
    )
    assert status == 400, body
    assert "DeepSeek" in body["error"]["message"], "要点名是哪一行不支持"

    status, task = api.call(
        "POST",
        "/api/checkin/tasks",
        {"name": " 早上一轮 ", "time": "7:05", "platform_ids": [platform_id, platform_id]},
    )
    assert status == 201, task
    assert task["time"] == "07:05", "落库前补齐 HH:MM"
    assert task["name"] == "早上一轮" and task["enabled"] is True
    assert task["platform_ids"] == [platform_id], "重复账号只留一份"

    # 没登录的账号：跑一次只会如实说未登录，不会发请求
    status, ran = api.call("POST", f"/api/checkin/tasks/{task['id']}/run")
    assert status == 200, ran
    assert ran["message"] == "1 未登录", ran["message"]
    assert ran["task"]["last_message"] == ran["message"]
    assert ran["task"]["last_run_day"], "跑过就要按天记账，否则会每分钟重跑"

    # 有会话之后：假密文 → 领取失败，账号状态与任务汇总各说各的
    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(INSERT_ACCOUNT, (platform_id,))
    db.commit()
    db.close()

    status, ran = api.call("POST", f"/api/checkin/tasks/{task['id']}/run")
    assert status == 200, ran
    assert ran["message"] == "1 领取失败", ran["message"]
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["accounts"][0]["status"] == "error"
    assert "登录态" in body["accounts"][0]["message"]

    status, updated = api.call(
        "PUT",
        f"/api/checkin/tasks/{task['id']}",
        {"name": "睡前", "time": "23:59", "platform_ids": [platform_id], "enabled": False},
    )
    assert status == 200, updated
    assert (updated["name"], updated["time"], updated["enabled"]) == ("睡前", "23:59", False)
    assert updated["last_run_day"] == ran["task"]["last_run_day"], "改配置不该抹掉今天的记账"

    # 退出登录：账号从清单里消失，任务还在，但跑起来是「1 未登录」
    status, _ = api.call("DELETE", f"/api/checkin/platforms/{platform_id}/session")
    assert status == 204
    status, ran = api.call("POST", f"/api/checkin/tasks/{task['id']}/run")
    assert ran["message"] == "1 未登录", ran["message"]

    # 删平台：任务不能继续引用一个已经不存在的账号
    status, _ = api.call("DELETE", f"/api/platforms/{platform_id}")
    assert status == 204
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["tasks"][0]["platform_ids"] == [], "已删除的账号要从任务里剔除"

    status, _ = api.call("DELETE", f"/api/checkin/tasks/{task['id']}")
    assert status == 204
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["tasks"] == []
    status, body = api.call("DELETE", f"/api/checkin/tasks/{task['id']}")
    assert status == 404, body


DIALOG = "[role=dialog]"


def _click(scope: str, text: str, aria: str | None = None) -> None:
    """Mantine 弹窗里的按钮：headless 下真实点击到不了 React，只能 element.click()。"""

    def run(page) -> None:
        found = page.evaluate(
            """({ scope, text, aria }) => {
                const root = document.querySelector(scope);
                if (!root) throw new Error(`没有 ${scope}`);
                const nodes = [...root.querySelectorAll("button")];
                const hit = aria
                    ? nodes.find(el => el.getAttribute("aria-label") === aria)
                    : nodes.find(el => el.textContent.trim().includes(text));
                if (!hit) {
                    throw new Error(
                        `${scope} 里找不到 ${aria || text}：` +
                        nodes.map(el => el.textContent.trim() || el.getAttribute("aria-label")).join(" | "),
                    );
                }
                hit.click();
                return true;
            }""",
            {"scope": scope, "text": text, "aria": aria},
        )
        assert found

    return run


def test_checkin_task_modal_is_desktop_only_ui(instances, browser_factory):
    """「定时领取」弹窗：只有桌面端才渲染，所以这里伪造 /api/auth/status 的 checkin_login。

    独立服务器模式没有登录窗口，`showCheckin` 永远是 false——不伪造就永远测不到这条路径，
    而这个弹窗恰恰是用户配置任务的地方，不能只靠接口测试。
    账号用假密文：能建任务，「立即运行」必然解密失败 → 汇总写「1 领取失败」，且不发真实请求。
    """
    instance = instances("checkin-task-ui")
    api = instance.api
    api.setup_admin()

    status, platform = api.call(
        "POST",
        "/api/platforms",
        {
            "name": "ModelScope 魔搭",
            "type": "OpenAI",
            "base_url": "https://api-inference.modelscope.cn/v1",
            "api_key": "sk-e2e-modelscope-key",
        },
    )
    assert status == 201, platform
    platform_id = platform["id"]

    db = sqlite3.connect(instance.dir / "data" / "ai-gateway.db")
    db.execute(INSERT_ACCOUNT, (platform_id,))
    db.commit()
    db.close()

    console = browser_factory(api)
    page = console.new_page()
    pageerrors: list[str] = []
    page.on("pageerror", lambda error: pageerrors.append(str(error)))

    def fake_desktop(route):
        body = route.fetch().json()
        body["checkin_login"] = True
        route.fulfill(content_type="application/json", body=json.dumps(body))

    page.route("**/api/auth/status", fake_desktop)
    console.goto(page, "/providers", ".console-shell")

    # 桌面端信号造出来之后，头部才出现「定时领取」
    page.wait_for_function(
        "() => [...document.querySelectorAll('button')].some(el => el.textContent.includes('定时领取'))",
        timeout=15000,
    )
    _click(".console-page", "定时领取")(page)
    page.wait_for_selector(DIALOG, timeout=10000)
    assert "到点自动为选中的账号领取积分" in page.inner_text(DIALOG)
    assert "还没有定时任务" in page.inner_text(DIALOG), "新库应当是空状态"

    _click(DIALOG, "新建任务")(page)
    # 默认草稿：00:05 + 所有已登录账号，直接保存
    _click(DIALOG, "保存任务")(page)
    page.wait_for_function(
        f"() => document.querySelector('{DIALOG}')?.textContent.includes('00:05')",
        timeout=15000,
    )
    listing = page.inner_text(DIALOG)
    assert "ModelScope 魔搭 · e2e-user" in listing, "账号列应当翻译成人看的名字"
    assert "尚未运行" in listing

    _click(DIALOG, "", aria="立即运行")(page)
    page.wait_for_function("() => document.body.innerText.includes('1 领取失败')", timeout=20000)
    # 列表刷新后，任务行要把这次的结果显示出来
    page.wait_for_function(
        f"() => document.querySelector('{DIALOG}')?.textContent.includes('1 领取失败')",
        timeout=15000,
    )

    # 停用开关：关掉之后「下次」这一栏要说停用，而不是继续报时间
    page.evaluate(f"""() => document.querySelector('{DIALOG} .mantine-Switch-root input').click()""")
    page.wait_for_function(
        f"() => document.querySelector('{DIALOG}')?.textContent.includes('已停用')",
        timeout=15000,
    )

    # 审计在「表格 + 已停用行」的状态下做：这一屏是用户最常看到的形状
    shot = REPO / ".dsh-tmp" / "checkin"
    shot.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(shot / "task-modal.png"))
    assert console.audit(page) == [], "弹窗打开状态下 axe 不许有违规"

    # 删除：回到空状态，接口那边也确认任务真的没了
    _click(DIALOG, "", aria="删除")(page)
    page.wait_for_function(
        f"() => document.querySelector('{DIALOG}')?.textContent.includes('还没有定时任务')",
        timeout=15000,
    )
    status, body = api.call("GET", "/api/checkin/accounts")
    assert body["tasks"] == []

    assert pageerrors == []
