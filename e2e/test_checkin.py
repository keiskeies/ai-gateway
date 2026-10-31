"""每日领取接口：平台即账号。

登录窗口只有 Tauri 桌面端才有，E2E 起的是独立服务，所以这里钉的是：
- 没有桌面壳时登录接口明确 409，能力表如实标记 login_supported=false；
- 会话绑在 platform_id 上，删平台要级联删掉会话；
- 假密文领取落成账号状态 error，不是接口 500；run-all 不因单个账号失败而整体报错。
"""

from __future__ import annotations

import sqlite3

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
