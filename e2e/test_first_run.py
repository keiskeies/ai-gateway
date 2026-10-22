"""§11.8 流程①：登录 / 初始化 → 首次配置向导。

用一份全新的库（独立实例），否则会被同 session 里别的用例建的 fixture 污染：
向导的自动进入条件是平台、虚拟模型、Key 三者都为空。
"""

ADMIN = "e2e-first-run"
PASSWORD = "e2e-first-run-pw"


def test_setup_then_wizard(instances, browser_factory, mock_upstream):
    instance = instances("first-run")
    api = instance.api
    console = browser_factory(api, init="localStorage.setItem('locale', 'zh')")
    page = console.new_page()
    problems: list[str] = []
    page.on("pageerror", lambda error: problems.append(str(error)))

    page.goto(f"{api.base}/?v=1#/", wait_until="domcontentloaded")
    page.wait_for_selector(".auth-card", timeout=15000)
    assert "初始化管理员账号" in page.inner_text("body")

    # 初始化态有三个字段：用户名、密码、确认密码（Mantine 不保证 type=text，按 autocomplete 定位）
    page.fill(".auth-card input[autocomplete=username]", ADMIN)
    passwords = page.locator(".auth-card input[autocomplete=new-password]")
    passwords.nth(0).fill(PASSWORD)
    passwords.nth(1).fill(PASSWORD)
    page.evaluate("() => document.querySelector('.auth-card button[type=submit]').click()")

    # 初始化后 Overview 自动进向导（库为空 + localStorage.wizard 未设）
    page.wait_for_function("() => location.hash === '#/setup'", timeout=20000)
    page.wait_for_selector(".console-wizard-steps", timeout=15000)
    assert page.locator(".console-wizard-segment").count() == 4
    assert "0 / 4 步已完成" in page.inner_text(".console-wizard-progress")

    # 管理员是浏览器表单建的，接口这边要另外登录才有会话
    api.login(ADMIN, PASSWORD)
    # 完成度按库派生，不是本地进度游标：四步全用接口做，向导应当照样认
    status, platform = api.call(
        "POST", "/api/platforms", {"name": "e2e-wizard", "type": "OpenAI", "base_url": mock_upstream, "api_key": "mock-key"}
    )
    assert status == 201, f"{status} {platform}"
    api.call("POST", f"/api/platforms/{platform['id']}/models/sync")
    status, proxy = api.call("POST", "/api/proxies", {"name": "e2e-wizard"})
    assert status == 201, f"{status} {proxy}"
    status, route = api.call(
        "POST",
        f"/api/proxies/{proxy['id']}/routes",
        {"backends": [{"platform_id": platform["id"], "model_id": "mock-mini"}]},
    )
    assert status == 201, f"{status} {route}"
    status, key = api.call("POST", "/api/api-keys", {"name": "e2e-wizard", "key": "sk-e2e-wizard-fixed-key-12345678"})
    assert status == 201, f"{status} {key}"
    status, _ = api.call(
        "POST",
        "/v1/chat/completions",
        {"model": "e2e-wizard", "messages": [{"role": "user", "content": "向导第四步"}]},
        token="sk-e2e-wizard-fixed-key-12345678",
    )
    assert status == 200, f"网关请求失败 {status}"
    api.wait_stats()

    page.reload(wait_until="domcontentloaded")
    page.wait_for_selector(".console-wizard-progress", timeout=15000)
    assert "4 / 4 步已完成" in page.inner_text(".console-wizard-progress")

    # 跳过向导：写 localStorage，之后不再自动弹出（第一个按钮是"退出到总览"，只写 seen）
    console.click_text(page, "跳过向导")
    page.wait_for_function("() => location.hash === '#/traffic'", timeout=10000)
    assert page.evaluate("() => localStorage.getItem('wizard')") == "skipped"
    page.goto(f"{api.base}/?v=2#/", wait_until="domcontentloaded")
    page.wait_for_selector(".console-live", timeout=15000)
    assert page.evaluate("() => location.hash") != "#/setup"

    assert problems == []
    page.close()
