"""§11.8 流程②：虚拟模型「添加后端」的模型 ID 候选。

候选必须来自平台的模型目录 / 远端列表，而不是按平台名匹配的内置预设：
自建平台名（如 agnes1）对不上任何预设，只靠预设就等于没有候选，
用户只能盲打模型 ID —— 这正是 1.3.1 重写后丢掉的行为。

下拉是 portal 到 body 的，页面上同时存在路由配置里那几个 Select 的隐藏选项，
所以选项一律按"可见"过滤后再按文本点，不能用共享 select_option 里"取第一个 role=option"的写法。
"""

import json

from conftest import AXE_JS

PLATFORM_SELECT = "[role=dialog] .mantine-Select-input"
MODEL_INPUT = "[role=dialog] .mantine-Autocomplete-input"

VISIBLE_OPTIONS = """() => [...document.querySelectorAll('[role=option]')]
    .filter(el => el.offsetParent !== null)
    .map(el => el.textContent.trim())"""


def _open_add_backend(page, console, proxy_id: str) -> None:
    console.goto(page, f"/models/{proxy_id}")
    console.click_text(page, "添加后端")
    page.wait_for_selector("[role=dialog]", timeout=10000)
    page.wait_for_function(
        "() => document.querySelector('[role=dialog]').innerText.includes('PLATFORM')", timeout=10000
    )


def _select_platform(page, name: str) -> None:
    page.wait_for_selector(PLATFORM_SELECT, timeout=10000)
    page.evaluate("(sel) => document.querySelector(sel).focus()", PLATFORM_SELECT)
    page.keyboard.type(name, delay=25)
    page.wait_for_function(
        """([name]) => [...document.querySelectorAll('[role=option]')]
            .some(el => el.offsetParent !== null && el.textContent.trim() === name)""",
        arg=[name],
        timeout=10000,
    )
    page.evaluate(
        """(name) => [...document.querySelectorAll('[role=option]')]
            .filter(el => el.offsetParent !== null)
            .find(el => el.textContent.trim() === name).click()""",
        name,
    )
    page.wait_for_function(
        "() => !document.querySelector('[role=dialog] .mantine-Autocomplete-input').disabled", timeout=10000
    )


def _open_model_dropdown(page) -> list[str]:
    page.evaluate("(sel) => document.querySelector(sel).focus()", MODEL_INPUT)
    page.wait_for_function(
        "() => [...document.querySelectorAll('[role=option]')].some(el => el.offsetParent !== null)",
        timeout=15000,
    )
    return page.evaluate(VISIBLE_OPTIONS)


def test_backend_model_offers_synced_and_remote_ids(console, page, api, errors, mock_stack):
    _open_add_backend(page, console, mock_stack["proxy"]["id"])
    # 平台名是 e2e 随机 tag，内置预设里必然没有，命中即证明候选不是从预设来的
    _select_platform(page, mock_stack["tag"])
    options = _open_model_dropdown(page)
    assert "mock-mini" in options, f"模型候选里没有已同步的 mock-mini：{options}"

    page.evaluate(
        """() => [...document.querySelectorAll('[role=option]')]
            .filter(el => el.offsetParent !== null)
            .find(el => el.textContent.trim() === 'mock-mini').click()"""
    )
    page.wait_for_function(
        "() => document.querySelector('[role=dialog] .mantine-Autocomplete-input').value === 'mock-mini'",
        timeout=10000,
    )
    assert errors == []


def test_backend_model_still_accepts_typed_id(console, page, api, errors, mock_stack):
    _open_add_backend(page, console, mock_stack["proxy"]["id"])
    _select_platform(page, mock_stack["tag"])
    console.set_input(page, MODEL_INPUT, "totally-custom-model")
    page.wait_for_function(
        "() => document.querySelector('[role=dialog] .mantine-Autocomplete-input').value === 'totally-custom-model'",
        timeout=10000,
    )
    assert errors == []


def test_add_backend_dialog_passes_axe(console, page, api, errors, mock_stack):
    """无障碍套件只审六页正文，弹窗不在其中；这里补上。

    只审弹窗：页面背后那张后端卡片用 --text-3 画 `.idx`/`.sub`，深色下是 4.41:1，
    是这条用例引入之前就有的问题，不该让弹窗的用例替它背锅。
    """
    _open_add_backend(page, console, mock_stack["proxy"]["id"])
    _select_platform(page, mock_stack["tag"])
    page.wait_for_timeout(300)
    page.add_script_tag(path=str(AXE_JS))
    page.wait_for_function("() => window.axe !== undefined", timeout=10000)
    findings = page.evaluate(
        """() => {
            const dialog = document.querySelector('[role=dialog]');
            return axe.run(dialog, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] },
                resultTypes: ['violations'] }).then(r => r.violations.map(v => ({
                    id: v.id, impact: v.impact, count: v.nodes.length,
                    html: (v.nodes[0]?.html || '').slice(0, 160),
                })));
        }"""
    )
    if findings:
        print(json.dumps(findings, ensure_ascii=False, indent=2))
    assert findings == []
    assert errors == []