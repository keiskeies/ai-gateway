"""虚拟模型级「思考等级」：/v1/models 上报配置值 + 未带参数时注入默认等级。

两半都在真实链路上验：
- 元数据：`/v1/models` 的 `reasoning.efforts` / `default` 跟着路由配置走；
- 请求路径：客户端没带思考参数时上游收到默认等级，显式带了或显式关了都不被覆盖。

上游侧断言读假上游自己记下的请求体（`upstream_bodies`）——库里存的永远是入站正文，
转发出去的那份没有别的地方能看到。
"""

import time

import pytest

DEFAULT_EFFORTS = ["minimal", "low", "medium", "high", "xhigh"]


def _set_efforts(api, route_id: str, efforts: list[str], default: str | None):
    """改路由的思考等级配置。`default=None` = 这次不动默认值（协议：不传 = 维持原值）。"""
    body = {"reasoning_efforts": efforts}
    if default is not None:
        body["default_effort"] = default
    status, data = api.call("PUT", f"/api/routes/{route_id}", body)
    assert status == 200, f"{status} {data}"
    return data


def _enable_reasoning(api, stack) -> None:
    """给后端补 Reasoning 能力位。

    没这个能力位时 `/v1/models` 根本不输出 reasoning 段（`models_list` 按后端能力交集判定），
    也就没有可断言的档位列表 —— 真实虚拟模型接的是推理模型，这里对齐那个形态。
    """
    backend = stack["route"]["backends"][0]
    status, data = api.call("PUT", f"/api/backends/{backend['id']}", {"capabilities": ["Reasoning", "Tool"]})
    assert status == 200, f"设置能力位失败 {status} {data}"


def _model_entry(api, name: str) -> dict:
    """`/v1/models` 是公开端点（不需要网关 Key），按虚拟模型名取自己那一条。"""
    status, data = api.call("GET", "/v1/models")
    assert status == 200, f"{status} {data}"
    entries = [item for item in data["data"] if item["id"] == name]
    assert len(entries) == 1, f"列表里没有 {name}：{[i['id'] for i in data['data']]}"
    return entries[0]


def _chat(api, stack, **extra):
    payload = {"model": stack["model"], "messages": [{"role": "user", "content": "思考等级用例"}]}
    payload.update(extra)
    status, data = api.call("POST", "/v1/chat/completions", payload, token=stack["secret"])
    assert status == 200, f"{status} {data}"


def _route(api, stack) -> dict:
    status, routes = api.call("GET", f"/api/proxies/{stack['proxy']['id']}/routes")
    assert status == 200, routes
    return routes[0]


def _wait_route(api, stack, efforts: list[str], seconds: float = 10.0) -> dict:
    """等浏览器那一发 PUT 真正落库（用例与页面是两条连接，保存是异步的）。"""
    deadline = time.time() + seconds
    route = _route(api, stack)
    while time.time() < deadline and route.get("reasoning_efforts") != efforts:
        time.sleep(0.3)
        route = _route(api, stack)
    assert route.get("reasoning_efforts") == efforts, f"库里没等到 {efforts}：{route}"
    return route


# 路由配置面板。策略 Select 与思考等级多选同时在 DOM 里，未打开的那个选项宽高为 0，
# 所以点选项一律先按 getBoundingClientRect() 过滤可见项。
PANEL = ".console-route-config"
MULTI = f"{PANEL} .mantine-MultiSelect-input"
# 「保存路由」按钮的定位表达式（注意是表达式，不是函数：_save 里直接内联取值）
SAVE = """[...document.querySelectorAll('.console-route-actions button')]
    .find(b => b.textContent.trim() === '保存路由')"""


def _open_dropdown(page, selector: str) -> None:
    """打开下拉。这里必须是 element.click()：探针实测 focus() 打不开这个多选框
    （`Console.select_option` 那套 focus + 方向键是给只读 Select 写的）。"""
    page.wait_for_selector(selector, timeout=15000)
    page.evaluate("(s) => document.querySelector(s).click()", selector)
    page.wait_for_function(
        "() => [...document.querySelectorAll('[role=option]')].some"
        "(el => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; })",
        timeout=10000,
    )


def _default_input(page) -> str:
    """「默认思考等级」输入框的选择器，按 label 文本定位。

    面板里有两条 `.mantine-Select-input`（负载均衡策略在前面），按 class 取会拿错；
    `Field` 用 htmlFor/id 绑定，所以 label 是唯一可靠的锚点。id 来自 `useId`，
    跨 React 小版本格式不稳（本仓库 19.2 是 `_R_0_`），所以取到值后走引号包起来的
    属性选择器而不是 `#{id}`。
    """
    field_id = page.evaluate(
        """() => {
            const label = [...document.querySelectorAll('.console-route-config label')]
                .find(l => l.textContent.trim().startsWith('默认思考等级'));
            return label ? label.getAttribute('for') : null;
        }"""
    )
    assert field_id, "面板里没有「默认思考等级」字段"
    return f'[id="{field_id}"]'


def _default_value(page) -> str:
    return page.locator(_default_input(page)).input_value()


def _wait_default_value(page, expected: str, seconds: float = 10.0) -> None:
    deadline = time.time() + seconds
    while time.time() < deadline and _default_value(page) != expected:
        time.sleep(0.2)
    assert _default_value(page) == expected, f"默认等级期望 {expected!r}，实际 {_default_value(page)!r}"


def _click_option(page, text: str) -> None:
    clicked = page.evaluate(
        """(text) => {
            const target = [...document.querySelectorAll('[role=option]')].find(el => {
                const r = el.getBoundingClientRect();
                return r.width > 0 && r.height > 0 && el.textContent.trim() === text;
            });
            if (!target) return false;
            target.click();
            return true;
        }""",
        text,
    )
    assert clicked, f"可见下拉里没有 {text}"
    page.wait_for_timeout(150)


def _pills(page) -> list[str]:
    return page.evaluate(
        f"""() => [...document.querySelectorAll('{PANEL} .mantine-MultiSelect-pill')]
            .map(p => p.textContent.trim())"""
    )


def _save(page) -> None:
    saved = page.evaluate(f"() => {{ const b = {SAVE}; if (!b) return false; b.click(); return true; }}")
    assert saved, "没有「保存路由」按钮"


@pytest.fixture
def reasoning_ui(console, api, mock_stack):
    """带会话 Cookie 的浏览器，停在「路由配置」面板上。

    `console` 与 `mock_stack` 用的是同一个 session 实例（同一个 `api`），所以页面上
    看到的就是这条用例刚建的那份虚拟模型。
    """
    _enable_reasoning(api, mock_stack)
    page = console.new_page()
    console.goto(page, f"/models/{mock_stack['proxy']['id']}")
    page.wait_for_selector(MULTI, timeout=15000)
    yield page
    page.close()


def test_route_panel_sets_levels_and_default_end_to_end(reasoning_ui, api, mock_stack, errors, upstream_bodies):
    """界面这一路：勾档位（含自定义 max）+ 选默认 → 保存 → 上游真收到默认等级。

    只有 API 层用例会漏掉前端：报文字段名、`normalizeEfforts` 的归一顺序、以及
    「空串 = 清空默认值」这条协议，都只在浏览器里才走一遍。
    """
    page = reasoning_ui

    # 未勾选档位时，默认等级选不了，且占位文案告诉你原因
    # （placeholder 不是文本节点，只能读属性，inner_text 里找不到它）
    default_input = page.locator(_default_input(page))
    assert default_input.is_disabled()
    assert "先在上方选择" in default_input.get_attribute("placeholder")

    _open_dropdown(page, MULTI)
    _click_option(page, "low")
    _click_option(page, "high")
    # 自定义档位：敲进搜索框后它自己成为一条候选（Mantine 7 没有 creatable）
    page.keyboard.type("max", delay=40)
    _click_option(page, "max")
    assert set(_pills(page)) >= {"low", "high", "max"}, _pills(page)

    # 默认等级只有勾过的档位可选，选 high
    page.keyboard.press("Escape")
    _open_dropdown(page, _default_input(page))
    _click_option(page, "high")
    assert _default_value(page) == "high", "默认等级没选上"

    _save(page)

    # 落库 + RouteCache 回填：归一顺序必须是候选在前、自定义在后。
    # 保存后草稿由 invalidate() 的服务端回填接管，所以以接口为准，不赌 pill 的渲染时序。
    route = _wait_route(api, mock_stack, ["low", "high", "max"])
    assert route["default_effort"] == "high", route
    assert _pills(page) == ["low", "high", "max"], "界面回显的档位和库里不一致"
    assert _model_entry(api, mock_stack["model"])["reasoning"]["default"] == "high"

    # 不带思考参数的一笔 → 默认等级真的转发出去了
    _chat(api, mock_stack)
    assert upstream_bodies[-1].get("reasoning_effort") == "high", upstream_bodies
    assert errors == []


def test_clearing_default_effort_stops_injection(reasoning_ui, api, mock_stack, upstream_bodies):
    """界面上点 × 清空默认等级 → 保存发的是空串 → 未带参数不再注入思考字段。

    「不传 = 这次没动」与「空串 = 清空」的区分只在界面上点 × 才走得到：
    API 层用例里默认值要么带、要么整个字段缺席。
    """
    page = reasoning_ui
    _set_efforts(api, mock_stack["route"]["id"], ["low", "high", "max"], "high")
    page.reload(wait_until="domcontentloaded")
    page.wait_for_selector(MULTI, timeout=15000)
    _wait_default_value(page, "high")

    # 清空按钮是 Mantine 的 InputClearButton（CloseButton，class 带这个静态选择器）。
    # 面板里只有这一个 clearable 控件（策略 Select 是 allowDeselect={false}，多选没开 clearable），
    # 所以按类名就能唯一定位；合成点击要走 element.click() 才到得了 React。
    cleared = page.evaluate(
        f"""() => {{
            const clear = document.querySelector('{PANEL} .mantine-InputClearButton-root');
            if (!clear) return false;
            clear.click();
            return true;
        }}"""
    )
    assert cleared, "默认等级 Select 上没有清空按钮"
    _wait_default_value(page, "")

    _save(page)

    deadline = time.time() + 10
    route = _route(api, mock_stack)
    while time.time() < deadline and route.get("default_effort"):
        time.sleep(0.3)
        route = _route(api, mock_stack)
    assert not route["default_effort"], f"空串没被当成清空：{route}"
    # 列表还在，只是没有默认值了
    assert route["reasoning_efforts"] == ["low", "high", "max"], route
    entry = _model_entry(api, mock_stack["model"])
    assert entry["reasoning"]["efforts"] == ["low", "high", "max"], entry
    assert "default" not in entry["reasoning"], entry

    _chat(api, mock_stack)
    sent = upstream_bodies[-1]
    for key in ("reasoning_effort", "reasoning", "thinking"):
        assert key not in sent, f"清空默认值后不该再注入 {key}：{sent}"


@pytest.fixture
def reasoning_stack(api, mock_stack):
    """把 mock 虚拟模型配成「暴露 low / high / max，默认 high」（含自定义档位 max）。

    `mock_stack` 是 function 作用域：每条用例一套新的虚拟模型/路由/Key，用完即删，
    所以这里改配置不需要还原。
    """
    _enable_reasoning(api, mock_stack)
    _set_efforts(api, mock_stack["route"]["id"], ["low", "high", "max"], "high")
    return mock_stack


def test_unset_model_keeps_fixed_five_levels_and_injects_nothing(api, mock_stack, upstream_bodies):
    """没配置过的虚拟模型：`/v1/models` 仍是固定五档、不带 default，且不发思考参数。"""
    _enable_reasoning(api, mock_stack)

    entry = _model_entry(api, mock_stack["model"])
    assert entry["reasoning"]["efforts"] == DEFAULT_EFFORTS, entry
    assert "default" not in entry["reasoning"], entry

    _chat(api, mock_stack)
    assert len(upstream_bodies) == 1, upstream_bodies
    sent = upstream_bodies[0]
    for key in ("reasoning_effort", "reasoning", "thinking", "enable_thinking"):
        assert key not in sent, f"未配置的虚拟模型不该注入 {key}：{sent}"


def test_models_list_reflects_configured_efforts(api, reasoning_stack):
    """设置后：efforts 就是配置的那份（含自定义 max），default 一起上报。"""
    entry = _model_entry(api, reasoning_stack["model"])
    reasoning = entry["reasoning"]
    assert reasoning["efforts"] == ["low", "high", "max"], reasoning
    assert reasoning["default"] == "high", reasoning
    # 列表里有档位形状的值 → 继续对客户端声称支持 reasoning_effort
    assert "reasoning_effort" in entry["supported_parameters"], entry


def test_missing_reasoning_gets_default_level(api, reasoning_stack, upstream_bodies):
    """请求没带任何思考参数 → 上游收到默认等级 high。"""
    _chat(api, reasoning_stack)
    assert upstream_bodies[-1].get("reasoning_effort") == "high", upstream_bodies


def test_client_level_beats_gateway_default(api, reasoning_stack, upstream_bodies):
    """请求显式带了 low → 上游收到 low，默认值不覆盖客户端。"""
    _chat(api, reasoning_stack, reasoning_effort="low")
    assert upstream_bodies[-1].get("reasoning_effort") == "low", upstream_bodies


def test_client_explicit_disable_beats_gateway_default(api, reasoning_stack, upstream_bodies):
    """请求显式关闭（reasoning_effort: none）→ 上游收到关闭，而不是默认等级。"""
    _chat(api, reasoning_stack, reasoning_effort="none")
    sent = upstream_bodies[-1]
    assert sent.get("reasoning_effort") == "none", f"客户端的「关」必须赢：{sent}"


def test_default_must_be_an_exposed_level(api, mock_stack):
    """默认值不在暴露列表里 → 保存被拒（界面也拦，但后端是唯一的真话）。"""
    status, data = api.call(
        "PUT", f"/api/routes/{mock_stack['route']['id']}",
        {"reasoning_efforts": ["low", "high"], "default_effort": "xhigh"},
    )
    assert status == 400, f"期望 400，实际 {status} {data}"
    assert "xhigh" in str(data), data

    # 「默认关闭」与「未带参数时补一个等级」语义相反，同样拒掉
    status, data = api.call(
        "PUT", f"/api/routes/{mock_stack['route']['id']}",
        {"reasoning_efforts": ["low", "high", "none"], "default_effort": "none"},
    )
    assert status == 400, f"期望 400，实际 {status} {data}"

    # 两次都被拒 → 什么都没落库
    route = _route(api, mock_stack)
    assert route["reasoning_efforts"] == [], route
    assert not route["default_effort"], route


def test_only_none_level_drops_reasoning_effort_param(api, mock_stack):
    """只暴露「可以关」这一档：不再对客户端声称支持 reasoning_effort。"""
    _enable_reasoning(api, mock_stack)
    _set_efforts(api, mock_stack["route"]["id"], ["none"], None)
    entry = _model_entry(api, mock_stack["model"])
    assert entry["reasoning"]["efforts"] == ["none"], entry
    assert "reasoning_effort" not in entry["supported_parameters"], entry
