"""请求详情：正文采集原因、截断可见、JSON/SSE 结构化展示。

mock 上游返回固定的 OpenAI JSON / SSE；测试只关心“原始正文能否还原成可读内容”，
所以不依赖真实供应商。截断用例直接发一个超过 512KB 的请求，验证旧行为里的
“整份丢弃”已经变成“保存前缀并标记已截断”。
"""

import json
from pathlib import Path

import pytest

SHOTS = Path(__file__).resolve().parents[1] / ".dsh-tmp" / "payload"


@pytest.fixture
def capture_bodies(api):
    status, settings = api.call(
        "PUT", "/api/settings",
        {"capture_request_bodies": True, "capture_response_bodies": True},
    )
    assert status == 200, f"{status} {settings}"
    yield
    api.call("PUT", "/api/settings", {"capture_request_bodies": False, "capture_response_bodies": False})


def _open_request(console, page, api, stack) -> None:
    console.goto(page, f"/traffic?q={stack['tag']}")
    page.wait_for_function(
        "() => document.querySelectorAll('.console-table tbody tr').length === 1",
        timeout=15000,
    )
    page.evaluate("() => document.querySelector('.console-table tbody tr').click()")
    page.wait_for_selector(".console-payload-view", timeout=15000)


def _blocks(page) -> dict:
    return page.evaluate(
        """() => {
            const out = {};
            // textContent 包含折叠的 thinking/原始 JSON；截断到 20k 避免超大请求把结果搬回 Python
            const text = (el) => (el.textContent || '').replace(/\\s+/g, ' ').trim().slice(0, 20000);
            for (const block of document.querySelectorAll('.console-payload-block')) {
                const label = block.querySelector('summary')?.textContent.trim() ?? '';
                if (label.startsWith('请求正文')) {
                    out.request = text(block);
                    out.requestTruncated = label.includes('已截断');
                    const raw = block.querySelector('.console-payload-body');
                    out.requestRaw = raw ? raw.textContent.slice(0, 200) : '';
                }
                if (label.startsWith('响应正文')) {
                    out.response = text(block);
                    out.responseTruncated = label.includes('已截断');
                }
            }
            return out;
        }"""
    )


def test_json_request_and_response_are_structured(console, page, api, mock_stack, capture_bodies):
    status, _ = api.call(
        "POST", "/v1/chat/completions",
        {
            "model": mock_stack["model"],
            "messages": [
                {"role": "system", "content": "你是测试助手"},
                {"role": "user", "content": "把这段内容展示成对话"},
            ],
            "temperature": 0.2,
            "max_tokens": 64,
        },
        token=mock_stack["secret"],
    )
    assert status == 200
    api.wait_stats()

    _open_request(console, page, api, mock_stack)
    blocks = _blocks(page)
    assert "系统提示" in blocks["request"] and "你是测试助手" in blocks["request"], blocks
    assert "把这段内容展示成对话" in blocks["request"], blocks
    assert "temperature" in blocks["request"] and "0.2" in blocks["request"], blocks
    assert "原始 JSON" in blocks["request"], blocks

    assert "mock answer" in blocks["response"], blocks
    assert "prompt_tokens" in blocks["response"] and "12" in blocks["response"], blocks
    assert "completion_tokens" in blocks["response"] and "5" in blocks["response"], blocks
    assert "stop" in blocks["response"], blocks
    assert "原始 JSON" in blocks["response"], blocks
    SHOTS.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(SHOTS / "payload-structured.png"), full_page=True)


def test_sse_response_is_reassembled(console, page, api, mock_stack, capture_bodies):
    status, _ = api.call(
        "POST", "/v1/chat/completions",
        {
            "model": mock_stack["model"],
            "messages": [{"role": "user", "content": "流式答案"}],
            "stream": True,
        },
        token=mock_stack["secret"],
    )
    assert status == 200
    api.wait_stats()

    _open_request(console, page, api, mock_stack)
    blocks = _blocks(page)
    assert "流式" in blocks["response"], blocks
    assert "mock answer" in blocks["response"], blocks
    assert "事件数" in blocks["response"], blocks
    assert "原始 SSE" in blocks["response"], blocks


def test_capture_off_explains_why_there_is_no_body(console, page, api, mock_stack):
    # fixture 之外的用例：这里刻意保持采集关闭
    status, settings = api.call(
        "PUT", "/api/settings",
        {"capture_request_bodies": False, "capture_response_bodies": False},
    )
    assert status == 200, f"{status} {settings}"
    try:
        status, _ = api.call(
            "POST", "/v1/chat/completions",
            {"model": mock_stack["model"], "messages": [{"role": "user", "content": "没有采集"}]},
            token=mock_stack["secret"],
        )
        assert status == 200
        api.wait_stats()

        console.goto(page, f"/traffic?q={mock_stack['tag']}")
        page.wait_for_function(
            "() => document.querySelectorAll('.console-table tbody tr').length === 1",
            timeout=15000,
        )
        page.evaluate("() => document.querySelector('.console-table tbody tr').click()")
        page.wait_for_selector(".console-payload", timeout=15000)
        page.wait_for_function(
            """() => {
                const blocks = [...document.querySelectorAll('.console-payload-block')];
                const req = blocks.find(b => b.querySelector('summary')?.textContent.includes('请求正文'));
                return req && req.innerText.includes('采集未开启');
            }""",
            timeout=15000,
        )
        text = page.inner_text(".console-payload")
        assert "设置 → 数据维护" in text and "历史请求无法补录" in text, text
    finally:
        api.call("PUT", "/api/settings", {"capture_request_bodies": False, "capture_response_bodies": False})


def test_oversized_request_keeps_a_visible_prefix(console, page, api, mock_stack, capture_bodies):
    # 超过 512KB：以前整份丢弃，现在保存前缀并在 UI 标明已截断
    huge = "x" * (600 * 1024)
    status, _ = api.call(
        "POST", "/v1/chat/completions",
        {"model": mock_stack["model"], "messages": [{"role": "user", "content": huge}]},
        token=mock_stack["secret"],
    )
    assert status == 200
    api.wait_stats()

    _, listing = api.call("GET", "/api/stats/requests?page=1&page_size=10")
    row = next(i for i in listing["items"] if i["proxy_name"] == mock_stack["tag"])
    assert row["has_body"] is False, "截断正文不能露出重放入口"

    _open_request(console, page, api, mock_stack)
    blocks = _blocks(page)
    assert blocks["requestTruncated"] is True, blocks
    assert "内容超过采集上限" in blocks["request"], blocks
    assert '"messages"' in blocks["requestRaw"], blocks


def _stub_payload(page, payload: dict) -> None:
    page.route(
        "**/api/stats/requests/*/payload",
        lambda route: route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps(payload, ensure_ascii=False),
        ),
    )


def _anthropic_payload(request_body: dict, response_body: str, content_type: str) -> dict:
    return {
        "request": {
            "headers_captured": False,
            "body_captured": True,
            "protocol": "anthropic",
            "headers": None,
            "body": json.dumps(request_body, ensure_ascii=False),
            "bytes": len(json.dumps(request_body, ensure_ascii=False)),
            "headers_truncated": False,
            "body_truncated": False,
        },
        "response": {
            "captured": True,
            "protocol": "anthropic",
            "status_code": 200,
            "content_type": content_type,
            "body": response_body,
            "bytes": len(response_body),
            "truncated": False,
        },
        "request_capture_enabled": True,
        "response_capture_enabled": True,
    }


def _prepare_stub_row(console, page, api, mock_stack, payload: dict) -> None:
    status, _ = api.call(
        "POST", "/v1/chat/completions",
        {"model": mock_stack["model"], "messages": [{"role": "user", "content": "占位"}]},
        token=mock_stack["secret"],
    )
    assert status == 200
    api.wait_stats()
    # 页面会自动选中第一行并立刻请求 payload：路由必须先装好，再导航
    _stub_payload(page, payload)
    console.goto(page, f"/traffic?q={mock_stack['tag']}")
    page.wait_for_function(
        "() => document.querySelectorAll('.console-table tbody tr').length === 1",
        timeout=15000,
    )
    page.wait_for_selector(".console-payload-view", timeout=15000)


def test_anthropic_json_blocks_are_structured(console, page, api, mock_stack):
    request_body = {
        "model": "claude-x",
        "max_tokens": 1024,
        "system": "你是 Claude",
        "tools": [{
            "name": "lookup",
            "description": "查询资料",
            "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}},
        }],
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "帮我查一下"},
                {"type": "tool_result", "tool_use_id": "toolu_prev", "content": "上次结果"},
            ],
        }],
    }
    response_body = json.dumps({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-x",
        "content": [
            {"type": "thinking", "thinking": "先想一想"},
            {"type": "text", "text": "最终回答"},
            {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"q": "天气"}},
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 100, "output_tokens": 20, "cache_read_input_tokens": 30},
    }, ensure_ascii=False)
    _prepare_stub_row(console, page, api, mock_stack, _anthropic_payload(request_body, response_body, "application/json"))

    blocks = _blocks(page)
    assert "你是 Claude" in blocks["request"] and "系统提示" in blocks["request"], blocks
    assert "帮我查一下" in blocks["request"], blocks
    assert "上次结果" in blocks["request"] and "工具结果" in blocks["request"], blocks
    assert "lookup" in blocks["request"] and "查询资料" in blocks["request"], blocks
    assert "max_tokens" in blocks["request"], blocks

    assert "最终回答" in blocks["response"], blocks
    assert "先想一想" in blocks["response"], blocks
    assert "lookup" in blocks["response"] and "天气" in blocks["response"], blocks
    assert "cache_read_input_tokens" in blocks["response"] and "30" in blocks["response"], blocks
    assert "tool_use" in blocks["response"], blocks


def test_anthropic_sse_deltas_are_reassembled(console, page, api, mock_stack):
    request_body = {"model": "claude-x", "messages": [{"role": "user", "content": "流式"}]}
    sse = "\n\n".join([
        'event: message_start\ndata: {"type":"message_start","message":{"model":"claude-x","usage":{"input_tokens":12}}}',
        'event: content_block_start\ndata: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}',
        'event: content_block_delta\ndata: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"先推理"}}',
        'event: content_block_start\ndata: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}',
        'event: content_block_delta\ndata: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"流式回答"}}',
        'event: message_delta\ndata: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}',
        'event: message_stop\ndata: {"type":"message_stop"}',
    ]) + "\n\n"
    _prepare_stub_row(console, page, api, mock_stack, _anthropic_payload(request_body, sse, "text/event-stream"))

    blocks = _blocks(page)
    assert "先推理" in blocks["response"], blocks
    assert "流式回答" in blocks["response"], blocks
    assert "end_turn" in blocks["response"], blocks
    assert "input_tokens" in blocks["response"] and "12" in blocks["response"], blocks
    assert "output_tokens" in blocks["response"] and "7" in blocks["response"], blocks
    assert "原始 SSE" in blocks["response"], blocks
