//! 控制台实时事件总线
//!
//! 为什么要它：控制台此前只能 30s 轮询，"实时"开关其实名不副实。热路径把统计写进
//! SQLite 之后没人通知前端，前端只能猜。这里加一条进程内广播，写入方发一次，
//! `/api/events` 的每个订阅者各收一份。
//!
//! 三条约束：
//! - **发布永不阻塞发布者**：`broadcast` 在没有订阅者、或某个订阅者队列溢出时直接丢，
//!   统计写入器不能因为"没人看控制台"或被卡住的浏览器而变慢。
//! - **只发元数据**：事件里只有计数与 id，没有请求体、没有 key、没有思考内容。
//!   前端拿到计数后再走原有接口取数，SSE 不是数据通道，只是"该刷新了"的信号。
//! - **断线可恢复**：浏览器重连后是一次全量刷新，不补发历史事件，所以事件必须
//!   能被安全地丢、重复地收。

use serde::Serialize;
use tokio::sync::broadcast;

/// 队列长度：单用户桌面应用，控制台最多开两三个页签，溢出只发生在浏览器被冻结时
const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConsoleEvent {
    /// 一批请求统计刚落库。前端由此刷新计数类视图；`failures` 用来判断值不值得打扰。
    Requests { count: usize, failures: usize },
    /// 一次平台连通检测出了结果。带 `platform_id`，列表页只刷新那一行。
    PlatformCheck {
        platform_id: String,
        ok: bool,
        latency_ms: Option<u64>,
        model_count: Option<usize>,
        error: Option<String>,
    },
    /// 通知中心新出现了告警（或解决后又回归）。只是信号：铃铛据此重新拉一次
    /// `GET /api/alerts`，事件本身不带消息正文，所以断线丢事件也不会漏掉告警。
    Alerts { opened: usize, unread: usize },
}

impl ConsoleEvent {
    /// SSE 的 `event:` 名，同时也是 `?topics=` 的取值
    pub fn topic(&self) -> &'static str {
        match self {
            ConsoleEvent::Requests { .. } => "requests",
            ConsoleEvent::PlatformCheck { .. } => "platform_check",
            ConsoleEvent::Alerts { .. } => "alerts",
        }
    }
}

#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<ConsoleEvent>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    /// 非阻塞发布：没有订阅者时 `send` 返回 Err，这里直接吞掉
    pub fn publish(&self, event: ConsoleEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.sender.subscribe()
    }
}

/// 解析 `?topics=requests,platform_check`：不传或空串表示不过滤；传了就按字面过滤，
/// 写错名字就是什么都收不到——和 WHERE 条件不匹配一样，连接本身还活着（心跳照发）
pub fn accepts(raw: Option<&str>, topic: &str) -> bool {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let list: Vec<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if list.is_empty() {
        return true;
    }
    list.iter().any(|want| *want == topic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_reaches_a_subscriber_and_survives_none() {
        // 没有订阅者时发布不能 panic，也不能阻塞——这是热路径
        let hub = EventHub::new();
        hub.publish(ConsoleEvent::Requests { count: 1, failures: 0 });

        let mut rx = hub.subscribe();
        hub.publish(ConsoleEvent::Requests { count: 3, failures: 1 });
        let got = rx.recv().await.expect("event");
        assert_eq!(got, ConsoleEvent::Requests { count: 3, failures: 1 });
        assert_eq!(got.topic(), "requests");
    }

    #[test]
    fn wire_shape_is_tagged_with_the_topic() {
        // 前端按 event 名分派，payload 里再带一份 type 便于调试
        let json = serde_json::to_value(ConsoleEvent::PlatformCheck {
            platform_id: "p1".into(),
            ok: false,
            latency_ms: Some(12),
            model_count: None,
            error: Some("连接被拒绝".into()),
        })
        .unwrap();
        assert_eq!(json["type"], "platform_check");
        assert_eq!(json["platform_id"], "p1");
        assert_eq!(json["model_count"], serde_json::Value::Null);

        let alerts = serde_json::to_value(ConsoleEvent::Alerts { opened: 2, unread: 5 }).unwrap();
        assert_eq!(alerts["type"], "alerts");
        assert_eq!(alerts["opened"], 2);
        assert!(accepts(Some("requests,alerts"), "alerts"));
    }

    #[test]
    fn a_bad_topics_filter_never_silences_the_stream() {
        assert!(accepts(None, "requests"));
        assert!(accepts(Some("   "), "requests"));
        assert!(!accepts(Some("nope"), "requests"), "过滤条件不匹配就是收不到，不猜意图");
        assert!(accepts(Some("requests,platform_check"), "platform_check"));
        assert!(!accepts(Some("requests"), "platform_check"));
        assert!(accepts(Some(" requests , platform_check "), "requests"));
    }
}
