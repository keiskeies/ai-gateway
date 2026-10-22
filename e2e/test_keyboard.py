"""§8.8 / §12.7 键盘走查里机器能判定的那半：Tab 落点、焦点环、Modal 焦点困留。

axe 只管"有没有可访问名"，管不了"键盘能不能走通"。这条钉三件事：
① 每页连按 Tab，焦点必须每次真的换元素；Tab 序走完时焦点交给浏览器（activeElement 回到
   body），那是终点不是缺陷，所以按"这一页走完之前每个落点都合格"来断言，并要求每页至少 6 个落点；
② 每个停靠点都是可交互元素，且带可见焦点环（outline 或 box-shadow，二者之一即可）；
③ 弹窗打开后焦点进对话框、Tab 被困在框内、Esc 关掉后焦点回到触发它的按钮。
"读起来顺不顺"、Tab 顺序合不合逻辑仍然要人看——这条只保证链路断不了。
"""

import pytest

PAGES = [
    ("/traffic", ".console-shell"),
    ("/models", ".console-shell"),
    ("/providers", ".console-shell"),
    ("/keys", ".console-shell"),
    ("/settings", ".console-shell"),
    ("/wall", ".wall-root"),
]
INIT = "localStorage.setItem('locale', 'zh'); localStorage.setItem('wizard', 'skipped')"
TABS = 14

# 按 Tab 前给当前元素打标记，按完之后看标记还在不在——比"名字变了没"可靠，
# 两个同名按钮（比如每行的删除）本来就该是合法停靠点。
MARK = "() => { if (document.activeElement && document.activeElement !== document.body) document.activeElement.setAttribute('data-e2e-tab', '1') }"

FOCUS_PROBE = """() => {
    const el = document.activeElement;
    if (!el || el === document.body) return null;
    const stale = el.getAttribute('data-e2e-tab') === '1';
    el.removeAttribute('data-e2e-tab');
    const cs = getComputedStyle(el);
    const outlineVisible = cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0;
    const shadowVisible = !!cs.boxShadow && cs.boxShadow !== 'none';
    const interactive = ['a', 'button', 'input', 'select', 'textarea', 'summary'].includes(el.tagName.toLowerCase())
        || el.hasAttribute('tabindex')
        || ['button', 'link', 'textbox', 'combobox', 'checkbox', 'radio', 'tab', 'menuitem', 'switch', 'slider', 'option'].includes(el.getAttribute('role') || '');
    return {
        stale,
        interactive,
        ring: outlineVisible || shadowVisible,
        outline: `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}`,
        tag: el.tagName.toLowerCase(),
        role: el.getAttribute('role') || '',
        name: (el.getAttribute('aria-label') || el.textContent || el.getAttribute('placeholder') || '').trim().slice(0, 30),
        inDialog: !!el.closest('[role=dialog]'),
    };
}"""


@pytest.mark.parametrize("scheme", ["dark", "light"])
def test_tab_reaches_named_focusable_stops(api, mock_stack, browser_factory, scheme):
    """每页连按 14 次 Tab：焦点要真的移动、停在可交互元素上、焦点环可见。"""
    console = browser_factory(api, init=f"{INIT}; localStorage.setItem('theme', '{scheme}')")
    page = console.new_page()
    problems: list[str] = []
    stops: dict[str, int] = {}
    reached: dict[str, int] = {}

    for route, mount in PAGES:
        console.goto(page, route, mount)
        page.wait_for_timeout(700)
        page.evaluate("() => document.activeElement?.blur()")
        count = 0
        for i in range(TABS):
            page.evaluate(MARK)
            page.keyboard.press("Tab")
            page.wait_for_timeout(40)
            stop = page.evaluate(FOCUS_PROBE)
            if stop is None:
                # 焦点走到文档末尾之后交给浏览器（地址栏），不是缺陷——这一页的 Tab 序走完了
                break
            count += 1
            where = f"{scheme} {route} Tab#{i + 1}"
            stops[stop["tag"]] = stops.get(stop["tag"], 0) + 1
            if stop["stale"]:
                problems.append(f"{where} 焦点没动，还停在 {stop['tag']} {stop['name']!r}")
            if not stop["interactive"]:
                problems.append(f"{where} 停在非交互元素 {stop['tag']}[{stop['role']}] {stop['name']!r}")
            if not stop["ring"]:
                problems.append(f"{where} 焦点环不可见 outline={stop['outline']} {stop['tag']} {stop['name']!r}")
        reached[route] = count
        if count < 6:
            problems.append(f"{scheme} {route} 只有 {count} 个 Tab 停靠点")

    page.close()
    print(f"每页 Tab 停靠点数：{reached}；类型分布：{stops}")
    assert problems == [], "\n".join(problems[:20])


def test_modal_keeps_focus_and_esc_returns_to_trigger(console, page, errors):
    """弹窗：焦点进框、Tab 出不去、Esc 关掉后焦点回到"添加平台"。"""
    console.goto(page, "/providers")
    page.wait_for_timeout(700)
    # 合成点击不带焦点，先 focus 再 click，才能验证"关闭后焦点回到触发元素"
    page.evaluate(
        """() => {
            const btn = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === '添加平台');
            btn.setAttribute('data-e2e-trigger', '1');
            btn.focus();
            btn.click();
        }"""
    )
    page.wait_for_selector("[role=dialog]", timeout=10000)

    first = page.evaluate(FOCUS_PROBE)
    assert first and first["inDialog"], f"打开弹窗后焦点没进对话框：{first}"

    escaped = []
    for i in range(12):
        page.keyboard.press("Tab")
        page.wait_for_timeout(40)
        stop = page.evaluate(FOCUS_PROBE)
        if stop is None or not stop["inDialog"]:
            escaped.append(f"Tab#{i + 1} 跑到对话框外：{stop}")
    assert escaped == [], "\n".join(escaped)

    page.keyboard.press("Escape")
    page.wait_for_selector("[role=dialog]", state="detached", timeout=10000)
    back = page.evaluate(
        """() => {
            const el = document.activeElement;
            return { tag: el?.tagName.toLowerCase(), isTrigger: el?.getAttribute('data-e2e-trigger') === '1' };
        }"""
    )
    assert back["isTrigger"], f"Esc 后焦点没回到触发按钮：{back}"
    assert errors == []
