"""§11.8 流程⑥：通知中心——制造状态 → 铃铛计数 → 查看深链 → 状态消失后自动解决。

告警是"当前状态"而不是事件日志：读一次就扫一遍，条件没了就 resolve，
所以这条链路不需要后台定时器，也不需要手工清库。
"""

KEY_SECRET = "sk-e2e-expiring-fixed-key-1234567"


def test_alert_bell_deep_link_and_auto_resolve(console, page, api, errors):
    status, key = api.call(
        "POST", "/api/api-keys", {"name": "e2e-expiring", "key": KEY_SECRET, "expires_at": "2020-01-01"}
    )
    assert status == 201, f"{status} {key}"
    dedupe = f"key_expiry:{key['id']}"

    _, alerts = api.call("GET", "/api/alerts")
    assert any(item["dedupe_key"] == dedupe for item in alerts["items"]), alerts["items"]

    try:
        console.goto(page, "/traffic")
        page.wait_for_selector(".console-bell-badge", timeout=20000)
        console.click(page, ".console-bell")
        page.wait_for_selector(".console-alerts-dropdown", timeout=10000)
        assert "e2e-expiring" in page.inner_text(".console-alerts-dropdown")

        page.evaluate(
            """() => {
                const row = [...document.querySelectorAll('.console-alert')].find(el => el.innerText.includes('e2e-expiring'));
                row.querySelector('.console-alerts-action').click();
            }"""
        )
        page.wait_for_function(f"() => location.hash === '#/keys/{key['id']}'", timeout=15000)
        assert "e2e-expiring" in page.inner_text("body")
    finally:
        api.call("DELETE", f"/api/api-keys/{key['id']}")

    # 状态消失 → 下一次扫描自动 resolve，不靠手工清
    _, resolved = api.call("GET", "/api/alerts?resolved=true")
    entry = next((item for item in resolved["items"] if item["dedupe_key"] == dedupe), None)
    assert entry and entry.get("resolved_at"), resolved["items"][:3]
    assert errors == []
