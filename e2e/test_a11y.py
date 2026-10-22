"""§11.8 无障碍：用仓库里的 axe-core 跑机器可判定的 WCAG 2.x A/AA 规则。

axe 只覆盖机器能判定的那部分（可访问名、对比度、landmark、重复 id 等）；
§12.7 的键盘走查和"读起来顺不顺"仍然要人看，这条只是把能自动化的那半钉住。
页面都带数据审：假上游那一笔请求 + 一把 Key + 一个平台 + 一个虚拟模型，
空状态和满表格是两套 DOM，只审空的那半等于没审。
配色两套都审：填充面的文字色由 CSS token 分方案给，只审深色会漏掉浅色那一套。
"""

import json

PAGES = [
    ("/traffic", ".console-shell"),
    ("/models", ".console-shell"),
    ("/providers", ".console-shell"),
    ("/keys", ".console-shell"),
    ("/settings", ".console-shell"),
    # /wall 不在 AppShell 里，外壳信号是 .wall-root
    ("/wall", ".wall-root"),
]
INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"


def test_console_pages_pass_axe(console, page, api, errors, mock_stack, browser_factory):
    status, _ = api.call(
        "POST",
        "/v1/chat/completions",
        {"model": mock_stack["model"], "messages": [{"role": "user", "content": f"无障碍走查-{mock_stack['tag']}"}]},
        token=mock_stack["secret"],
    )
    assert status == 200, f"网关请求失败 {status}"
    api.wait_stats()

    light = browser_factory(api, init=f"{INIT}; localStorage.setItem('theme', 'light')")
    handles = [("深色", console, page), ("浅色", light, light.new_page())]
    findings: dict[str, list[dict]] = {}
    try:
        for scheme, owner, handle in handles:
            for route, mount in PAGES:
                owner.goto(handle, route, mount)
                # 首屏查询落地之后再审：否则审到的是骨架屏，表格里的可访问名根本没进 DOM
                handle.wait_for_timeout(800)
                found = owner.audit(handle)
                if found:
                    findings[f"{scheme} {route}"] = found
    finally:
        handles[1][2].close()

    if findings:
        print(json.dumps(findings, ensure_ascii=False, indent=2))
    assert findings == {}, f"{len(findings)} 个页面有 axe 违规"
    assert errors == []
