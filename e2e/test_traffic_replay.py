"""§11.8 流程③：流量筛选 → 打开详情 → 重放。

重放多出来的那条明细走实时通道的"有 N 条新请求，点击查看"进列表：统计是后台批量落库
（500ms 一刷），响应回来时那一行通常还没进表，硬刷一次只会刷出旧列表。
"""


def test_replay_from_inspector(console, page, api, errors, mock_stack):
    status, settings = api.call("PUT", "/api/settings", {"capture_request_bodies": True})
    assert status == 200 and settings.get("capture_request_bodies") is True, f"{status} {settings}"

    try:
        status, _ = api.call(
            "POST",
            "/v1/chat/completions",
            {"model": mock_stack["model"], "messages": [{"role": "user", "content": f"重放标记-{mock_stack['tag']}"}]},
            token=mock_stack["secret"],
        )
        assert status == 200, f"网关请求失败 {status}"
        api.wait_stats()

        # 按 tag 模糊搜（平台/虚拟模型/Key 同名）而不是按 model：别的用例也走同一个 mock 模型名
        console.goto(page, f"/traffic?q={mock_stack['tag']}")
        page.wait_for_function("() => document.querySelectorAll('.console-table tbody tr').length === 1", timeout=15000)
        page.evaluate("() => document.querySelector('.console-table tbody tr').click()")

        console.click_text(page, "重放")
        page.wait_for_selector("[role=dialog]", timeout=10000)
        dialog_text = page.inner_text("[role=dialog]")
        assert "重放这条请求？" in dialog_text
        assert "会产生真实费用" in dialog_text

        page.evaluate("() => [...document.querySelectorAll('[role=dialog] button')].pop().click()")
        page.wait_for_selector(".console-replay", timeout=30000)
        assert "mock answer" in page.inner_text(".console-replay-body")
        assert any("重放完成：HTTP 200" in text for text in console.toasts(page))

        # 重放行进列表靠提示条，不靠自动刷新：实时连接在的时候页面根本不轮询
        page.wait_for_selector(".console-livebar", timeout=15000)
        page.evaluate("() => document.querySelector('.console-livebar').click()")
        page.wait_for_selector(".console-replay-mark", timeout=15000)
        # 列表里只有 ↻ 标记，"重放自 #N" 在选中那一行后的详情里
        page.evaluate("() => document.querySelector('.console-table tbody tr').click()")
        page.wait_for_selector(".console-replayed-from", timeout=10000)
        assert "重放自 #" in page.inner_text(".console-replayed-from")

        api.wait_stats()
        _, listing = api.call("GET", "/api/stats/requests?page=1&page_size=10")
        items = listing["items"]
        # 按虚拟模型名认这一笔：mock 模型名在别的用例里也会出现
        origin = next(i for i in items if i["proxy_name"] == mock_stack["tag"] and i.get("replayed_from") is None)
        replay_row = next(i for i in items if i.get("replayed_from") == origin["id"])
        assert origin["has_body"] is True
        # 重放不另存正文，否则一次重放能无限复制正文
        assert replay_row["has_body"] is False

        _, again = api.call("POST", f"/api/stats/requests/{replay_row['id']}/replay")
        assert "no stored body" in str(again), again

        _, wiped = api.call("DELETE", "/api/stats/bodies")
        assert wiped.get("deleted", 0) >= 1, wiped
        _, after = api.call("GET", f"/api/stats/requests/{origin['id']}/trace")
        assert after.get("attempts"), after
    finally:
        api.call("PUT", "/api/settings", {"capture_request_bodies": False})

    assert errors == []
