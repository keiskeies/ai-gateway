"""§11.8 流程⑤：设置页的两道闸门——非回环地址警告、按年龄清理的预览确认。

只改表单值、不保存：真把 host 改成 0.0.0.0 会把这台预览实例暴露出去。
"""


def test_non_loopback_warning_and_purge_confirm(console, page, api, errors):
    console.goto(page, "/settings")
    host_input = '.console-settings-grid input[placeholder="127.0.0.1"]'
    page.wait_for_selector(host_input, timeout=15000)
    # 页面上本来就有别的 warn（清理说明），所以按文案认这一条，不数总数
    shown = "[...document.querySelectorAll('.console-note.warn')].some(el => el.textContent.includes('暴露到局域网'))"
    assert page.evaluate(f"() => {shown}") is False

    console.set_input(page, host_input, "0.0.0.0")
    page.wait_for_function(f"() => {shown}", timeout=10000)

    console.set_input(page, host_input, "127.0.0.1")
    page.wait_for_function(f"() => !({shown})", timeout=10000)

    before = api.call("GET", "/api/stats/db")[1]
    console.click_text(page, "按年龄清理")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    # 预览是 dry_run：确认框先把要删多少行说清楚，才允许执行
    page.wait_for_function("() => document.querySelector('[role=dialog]').innerText.includes('清理')", timeout=15000)
    assert page.inner_text("[role=dialog]")
    console.click_text(page, "取消")
    page.wait_for_selector("[role=dialog]", state="detached", timeout=10000)

    after = api.call("GET", "/api/stats/db")[1]
    assert after == before, f"取消清理却动了数据：{before} → {after}"
    assert errors == []
