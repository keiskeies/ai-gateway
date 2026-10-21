"""§11.8 流程⑦：整库备份——拍快照 → 还原 → 退回登录页。

还原会倒带整库并作废当前会话，所以这条用例独占一份实例，
绝不和别的用例共用那个库。
"""


def test_snapshot_restore_returns_to_login(instances, browser_factory):
    instance = instances("backup")
    api = instance.api
    api.setup_admin()
    console = browser_factory(api)
    page = console.new_page()
    problems: list[str] = []
    page.on("pageerror", lambda error: problems.append(str(error)))

    console.goto(page, "/settings")
    page.wait_for_function("() => document.body.innerText.includes('整库备份')", timeout=15000)
    assert "还没有" in page.inner_text("body") or "0" in page.inner_text("body")

    console.click_text(page, "立即备份")
    page.wait_for_function("() => document.body.innerText.includes('aigw-')", timeout=20000)
    _, listed = api.call("GET", "/api/backups")
    assert len(listed["items"]) == 1, listed

    # 快照之后新增的东西，还原后要消失
    status, key = api.call("POST", "/api/api-keys", {"name": "e2e-after-snapshot", "key": "sk-e2e-after-snapshot-key-1234"})
    assert status == 201, f"{status} {key}"

    console.click_text(page, "还原")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    page.wait_for_function("() => document.querySelector('[role=dialog]').innerText.includes('还原')", timeout=10000)
    page.evaluate("() => [...document.querySelectorAll('[role=dialog] button')].pop().click()")

    # 会话和库在同一个文件里，还原后必须被登出——否则"还原后会被登出"这句话是假的
    page.wait_for_selector(".auth-card", timeout=30000)
    _, auth = api.call("GET", "/api/auth/status")
    assert auth["authenticated"] is False, auth

    api.login()
    _, keys = api.call("GET", "/api/api-keys")
    assert all(k["name"] != "e2e-after-snapshot" for k in keys), keys
    _, backups = api.call("GET", "/api/backups")
    # 还原前自动拍一份安全快照，"退回去"这条路才真的能退
    assert len(backups["items"]) >= 2, backups

    assert problems == []
    page.close()
