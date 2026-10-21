"""§11.8 流程④：删除虚拟模型前的历史 / Key 影响确认。

确认框里的条数必须等于真正会被删掉的行数——接口与删除共用同一个 WHERE，
所以这里同时是那条口径的回归。
"""


def test_delete_model_discloses_history(console, page, api, errors, mock_stack):
    status, key = api.call("PUT", f"/api/api-keys/{mock_stack['key']['id']}", {"proxy_id": mock_stack["proxy"]["id"]})
    assert status == 200, f"{status} {key}"

    status, _ = api.call(
        "POST",
        "/v1/chat/completions",
        {"model": mock_stack["model"], "messages": [{"role": "user", "content": "删除前的一笔"}]},
        token=mock_stack["secret"],
    )
    assert status == 200, f"网关请求失败 {status}"
    api.wait_stats()

    _, impact = api.call("GET", f"/api/stats/proxy/{mock_stack['proxy']['id']}/impact")
    assert impact == {"requests": 1, "attempts": 1}, impact

    console.goto(page, f"/models/{mock_stack['proxy']['id']}")
    # 危险区在"设置"标签页里，默认停在路由页
    page.locator(".console-tabs [role=tab]").nth(3).evaluate("el => el.click()")
    page.wait_for_selector(".console-panel-title", timeout=10000)
    console.click_text(page, "删除虚拟模型")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    page.wait_for_function("() => document.querySelector('[role=dialog]').innerText.includes('条历史请求')", timeout=15000)
    dialog_text = page.inner_text("[role=dialog]")
    assert "1 条历史请求" in dialog_text and "1 条重试轨迹" in dialog_text, dialog_text
    assert "1 个 Key 绑定此模型" in dialog_text, dialog_text

    page.evaluate("() => [...document.querySelectorAll('[role=dialog] button')].pop().click()")
    page.wait_for_function("() => location.hash === '#/models'", timeout=15000)
    toasts = " · ".join(console.toasts(page))
    assert "已删除" in toasts, toasts
    assert "1 个绑定 Key 已一并停用" in toasts, toasts

    # 删除后：模型没了，Key 还在但被停用（不会变成全局 Key）
    _, keys = api.call("GET", "/api/api-keys")
    leftover = next(k for k in keys if k["id"] == mock_stack["key"]["id"])
    assert leftover["enabled"] is False and leftover["proxy_id"] == mock_stack["proxy"]["id"], leftover
    _, listing = api.call("GET", "/api/stats/requests?page=1&page_size=10")
    assert all(i["model_id"] != mock_stack["backend_model"] for i in listing["items"]), listing["items"][:2]

    assert errors == []
