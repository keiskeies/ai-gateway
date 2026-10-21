"""§11.8 流程②：添加平台 → 测试连接 → 虚拟模型 → Key → 试验台成功请求。

平台走 UI（创建弹窗 + 免落库的测试连接），模型/路由/Key 走接口——
路由编辑器本身另有专门用例覆盖，这里要证的是"从空库到一笔真实 200"这条链路。
上游是本地假服务，不花真钱。
"""

TAG = "e2e-modal"


def test_provider_modal_test_connection_then_playground(console, page, api, errors, mock_upstream):
    console.goto(page, "/providers")
    console.click_text(page, "添加平台")
    page.wait_for_selector("[role=dialog]", timeout=10000)

    console.set_input(page, '[role=dialog] input[placeholder="DeepSeek"]', TAG)
    console.set_input(page, '[role=dialog] input[placeholder="https://api.openai.com/v1"]', mock_upstream)
    console.set_input(page, '[role=dialog] input[placeholder="sk-..."]', "mock-key")

    # 未保存的配置也能测：接口不读平台表，正文里带 type/base_url/api_key
    console.click_text(page, "测试连接")
    page.wait_for_function("() => document.querySelector('[role=dialog]').innerText.includes('连接成功')", timeout=20000)
    assert "1 个模型" in page.inner_text("[role=dialog]")

    console.click_text(page, "新建")
    page.wait_for_function(f"() => document.body.innerText.includes('{TAG}')", timeout=15000)
    assert any("已添加" in text or "创建" in text for text in console.toasts(page))

    _, platforms = api.call("GET", "/api/platforms")
    created = next(p for p in platforms if p["name"] == TAG)
    # 掩码：接口回传的 api_key 不是明文（短 Key 会整段变点）
    assert "•" in created["api_key"] and "mock-key" not in created["api_key"], created["api_key"]

    api.call("POST", f"/api/platforms/{created['id']}/models/sync")
    status, proxy = api.call("POST", "/api/proxies", {"name": TAG})
    assert status == 201, f"{status} {proxy}"
    status, route = api.call(
        "POST",
        f"/api/proxies/{proxy['id']}/routes",
        {"backends": [{"platform_id": created["id"], "model_id": "mock-mini"}]},
    )
    assert status == 201, f"{status} {route}"
    status, key = api.call("POST", "/api/api-keys", {"name": TAG, "key": "sk-e2e-modal-fixed-key-12345678"})
    assert status == 201, f"{status} {key}"

    console.goto(page, "/playground")
    console.select_option(page, ".console-playground", TAG)
    console.set_input(page, '.console-playground input[placeholder="粘贴完整 Key（sk-aigw-…）"]', "sk-e2e-modal-fixed-key-12345678")
    console.set_field_by_label(page, ".console-playground", "试验台一笔", "message", "消息")
    console.click_text(page, "发送测试请求")

    page.wait_for_function("() => document.querySelector('.console-playground-response')?.innerText.includes('mock answer')", timeout=40000)
    api.wait_stats()
    status, listing = api.call("GET", "/api/stats/requests?page=1&page_size=5")
    assert any(item["model_id"] == "mock-mini" and item["status_code"] == 200 for item in listing["items"]), listing["items"][:2]

    assert errors == []
