//! 控制台实时事件流（SSE）
//!
//! 一个浏览器一条连接，`EventSource` 自带重连。这里只负责把 `EventHub` 的广播
//! 翻译成 SSE 帧，并每 20s 发一条注释行当心跳——没有心跳，空闲久了连接会被
//! 中间的 WebView/代理悄悄掐掉，而浏览器要等下一次读才发现，界面就停在旧数字上。

use actix_web::http::header;
use actix_web::{web, HttpResponse};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use std::time::Duration;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

use crate::events::{accepts, ConsoleEvent, EventHub};

/// 心跳间隔：只要小于常见 60s 空闲断连即可，太密只是白烧流量
const KEEPALIVE_SECS: u64 = 20;

#[derive(Deserialize)]
pub struct EventsQuery {
    /// 逗号分隔的事件名，如 `requests,platform_check`；不传表示全部
    pub topics: Option<String>,
}

/// 一帧 SSE：事件名走 `event:`，负载走 `data:`，空行结束
fn frame(event: &ConsoleEvent) -> Option<String> {
    let payload = serde_json::to_string(event).ok()?;
    Some(format!("event: {}\ndata: {}\n\n", event.topic(), payload))
}

fn comment(text: &str) -> String {
    format!(": {}\n\n", text)
}

pub async fn stream(hub: web::Data<EventHub>, query: web::Query<EventsQuery>) -> HttpResponse {
    let topics = query.topics.clone().unwrap_or_default();

    let events = BroadcastStream::new(hub.subscribe()).filter_map(move |item| {
        let topics = topics.clone();
        async move {
            match item {
                Ok(event) if accepts(Some(topics.as_str()), event.topic()) => frame(&event),
                Ok(_) => None,
                // 队列溢出（订阅者读不过来）：漏掉的事件补不回来，但也不能因此把连接关掉。
                // 发一条注释行，前端的全量兜底刷新负责把数字追平。
                Err(_) => Some(comment("lagged, please refetch")),
            }
        }
    });

    let pings = IntervalStream::new(tokio::time::interval(Duration::from_secs(KEEPALIVE_SECS)))
        .map(|_| comment("ping"));

    let body = futures_util::stream::select(events, pings)
        .map(|chunk| Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(chunk)));

    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "text/event-stream; charset=utf-8"))
        .insert_header((header::CACHE_CONTROL, "no-cache, no-transform"))
        .insert_header((header::CONNECTION, "keep-alive"))
        // 反代默认会缓冲 text/event-stream，这条头是让它别缓冲
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(Box::pin(body) as PinStream)
}

type PinStream = std::pin::Pin<Box<dyn Stream<Item = Result<actix_web::web::Bytes, actix_web::Error>> + 'static>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_carry_the_topic_as_event_name() {
        let f = frame(&ConsoleEvent::Requests { count: 2, failures: 1 }).expect("serializable");
        assert!(f.starts_with("event: requests\ndata: "), "{f}");
        assert!(f.ends_with("\n\n"), "{f}");
        assert!(f.contains("\"count\":2"), "{f}");
    }

    #[test]
    fn pings_and_lag_notes_are_comment_lines() {
        // SSE 里以冒号开头的行是注释，EventSource 不会当成消息触发 onmessage
        assert!(comment("ping").starts_with(": "));
        assert!(comment("lagged, please refetch").ends_with("\n\n"));
    }
}
