//! 端到端集成测试：私有协议上游 → 网关 → 客户端。
//!
//! 这些测试**起一个真实的 mock 上游 HTTP 服务**，把 Trae / WorkBuddy 的 SSE 形态
//! 原样吐出来，再让网关的 `PrivateSseNormalizer` 处理，验证「上游 SSE → OpenAI 形态」
//! 这条最容易出错、又最难靠单测覆盖的链路。
//!
//! 为什么要真起 HTTP：SSE 的分帧行为（一个事件跨两个 TCP 包）在纯函数测试里
//! 很难复现，而那正是「半截 JSON 被当成完整事件」这类 bug 的温床。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use crate::proxy::upstream::private_sse::{
    NormalizedEvent, PrivateSseNormalizer, ProtocolKind,
};

/// 起一个只回一次的 HTTP 服务，返回监听端口。
fn spawn_sse_server(body: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            handle_connection(stream, body);
        }
    });
    port
}

fn handle_connection(mut stream: TcpStream, body: String) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    // 读到请求头结束（空行）为止
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// 用裸 TCP 拉取 mock 上游（reqwest 没开 blocking feature，这里不需要它），
/// 返回响应正文。
fn fetch_body(port: u16) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect mock upstream");
    let request = "GET /v2/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).expect("write request");
    stream.flush().ok();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    // 跳过响应头，取正文
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(0);
    raw[split..].to_vec()
}

/// 用真实 HTTP 拉取 mock 上游，把字节喂给归一化器。
fn run_against_mock(body: String, kind: ProtocolKind) -> PrivateSseNormalizer {
    let port = spawn_sse_server(body);
    let bytes = fetch_body(port);
    let mut normalizer = PrivateSseNormalizer::new(kind, "test-model");
    normalizer.push(&bytes);
    normalizer
}

#[test]
fn workbuddy_sse_over_real_http_is_normalized() {
    // 模拟 WorkBuddy 的真实响应：多帧增量 + 尾帧 usage
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"，世界\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":5}}\n\n",
        "data: [DONE]\n\n",
    )
    .to_string();

    let normalizer = run_against_mock(body, ProtocolKind::WorkBuddy);
    assert_eq!(normalizer.text, "你好，世界");
    let response = normalizer.into_response();
    assert_eq!(response["choices"][0]["message"]["content"], "你好，世界");
    assert_eq!(response["choices"][0]["finish_reason"], "stop");
    assert_eq!(response["usage"]["prompt_tokens"], 12);
    assert_eq!(response["usage"]["completion_tokens"], 5);
}

#[test]
fn workbuddy_tool_call_over_http_gets_tool_calls_finish_reason() {
    // 上游的真实缺陷：发 tool_calls 时 finish_reason 仍是 stop
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"北京\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    )
    .to_string();

    let normalizer = run_against_mock(body, ProtocolKind::WorkBuddy);
    let response = normalizer.into_response();
    assert_eq!(
        response["choices"][0]["finish_reason"], "tool_calls",
        "有 tool_calls 时必须改写 finish_reason，否则客户端不执行工具"
    );
    let call = &response["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(call["id"], "call_abc");
    assert_eq!(call["function"]["name"], "get_weather");
    assert_eq!(call["function"]["arguments"], "{\"city\":\"北京\"}");
}

#[test]
fn trae_sse_over_real_http_is_normalized() {
    let body = concat!(
        "data: {\"event\":\"output\",\"output\":{\"reasoning_content\":\"先想一想\"}}\n\n",
        "data: {\"event\":\"output\",\"output\":{\"content\":\"答案是 42\"}}\n\n",
        "data: {\"event\":\"done\"}\n\n",
    )
    .to_string();

    let normalizer = run_against_mock(body, ProtocolKind::Trae);
    assert_eq!(normalizer.text, "答案是 42");
    assert_eq!(normalizer.reasoning, "先想一想");
    let response = normalizer.into_response();
    assert_eq!(response["choices"][0]["message"]["content"], "答案是 42");
    assert_eq!(
        response["choices"][0]["message"]["reasoning_content"],
        "先想一想"
    );
}

#[test]
fn business_error_in_sse_is_detected_over_http() {
    // Trae / WorkBuddy 都可能把错误塞在 200 的 SSE 里
    let body = "data: {\"code\":4008,\"msg\":\"quota exceeded\"}\n\n".to_string();
    let port = spawn_sse_server(body);
    let bytes = fetch_body(port);

    let mut normalizer = PrivateSseNormalizer::new(ProtocolKind::Trae, "m");
    let events = normalizer.push(&bytes);
    let mut found = None;
    for event in events {
        if let NormalizedEvent::BusinessError(err) = event {
            found = Some(err);
        }
    }
    let err = found.expect("必须识别出业务错误");
    assert_eq!(err.code, Some(4008));
    assert_eq!(err.message, "quota exceeded");
}

#[test]
fn split_tcp_frames_do_not_break_parsing() {
    // 关键回归：一个 SSE 事件跨两个 TCP 包到达时，不能把半截 JSON 当完整事件。
    // 这正是真实网络里最常见的情况，也是最容易写错的地方。
    let full = "data: {\"choices\":[{\"delta\":{\"content\":\"分帧\"},\"finish_reason\":null}]}\n\n";
    let split_at = full.len() / 2;
    let (head, tail) = full.split_at(split_at);

    let mut normalizer = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
    let first = normalizer.push(head.as_bytes());
    assert!(
        first.is_empty(),
        "半截事件不该产出任何 chunk（否则客户端会收到残缺 JSON）"
    );
    normalizer.push(tail.as_bytes());
    assert_eq!(normalizer.text, "分帧");
}

#[test]
fn empty_sse_produces_empty_response_without_panic() {
    let body = String::new();
    let normalizer = run_against_mock(body, ProtocolKind::WorkBuddy);
    assert_eq!(normalizer.text, "");
    // 仍然要能产出一个合法响应（content 为 null），而不是 panic
    let response = normalizer.into_response();
    assert_eq!(response["object"], "chat.completion");
    assert!(response["choices"][0]["message"]["content"].is_null());
}

#[test]
fn heartbeat_only_sse_is_survivable() {
    let body = ": ping\n\n: ping\n\n".to_string();
    let normalizer = run_against_mock(body, ProtocolKind::Trae);
    assert_eq!(normalizer.text, "");
    let response = normalizer.into_response();
    assert_eq!(response["choices"][0]["finish_reason"], "stop");
}
