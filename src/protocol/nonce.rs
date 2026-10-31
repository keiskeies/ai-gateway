//! 请求前置随机串：让上游看到的前缀每次都不同（字母数字混合）。
//!
//! 「最前面」在两种目标协议上不是同一个字段——Anthropic 把 system 放在顶层，
//! OpenAI 兼容上游只渲染 messages。所以注入位置由调用方按目标协议选 `Surface`。

use super::types::{Role, UnifiedMessage, UnifiedRequest};

/// 注入位置：目标是哪个「真正会被渲染」的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Anthropic 目标、私有协议适配器：读 `unified.system`
    SystemField,
    /// OpenAI 兼容目标：只渲染 `unified.messages`
    Messages,
}

/// 随机串的字符集：数字 + 大小写英文字母。
const NONCE_CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 32..=128 位字母数字串，长度与内容都随机。
pub(crate) fn random_nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let len = rng.gen_range(32..=128);
    (0..len)
        .map(|_| NONCE_CHARSET[rng.gen_range(0..NONCE_CHARSET.len())] as char)
        .collect()
}

/// 在整段 prompt 的最前面插入随机数字串，并返回生成的串（供详情页展示）。
/// 每次上游尝试调用一次。
pub fn apply(unified: &mut UnifiedRequest, surface: Surface) -> String {
    let nonce = random_nonce();
    match surface {
        Surface::SystemField => {
            if let Some(system) = unified.system.as_mut() {
                *system = format!("{nonce}\n{system}");
                return nonce;
            }
            insert_leading_user(unified, nonce.clone());
        }
        Surface::Messages => {
            if let Some(first) = unified.messages.first_mut() {
                if first.role == Role::System && prefix_content(&mut first.content, &nonce) {
                    return nonce;
                }
            }
            // Anthropic 入口的 system 只在顶层字段，openai::to_request 会把它补成 messages[0]；
            // 加在这里才不会被 system 盖在后面。
            if let Some(system) = unified.system.as_mut() {
                *system = format!("{nonce}\n{system}");
                return nonce;
            }
            insert_leading_user(unified, nonce.clone());
        }
    }
    nonce
}

fn insert_leading_user(unified: &mut UnifiedRequest, nonce: String) {
    unified.messages.insert(0, UnifiedMessage {
        role: Role::User,
        content: Some(serde_json::Value::String(nonce)),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning_blocks: Vec::new(),
        raw_blocks: Vec::new(),
    });
}

/// 内容是字符串或内容块数组；数组时拼到第一个 text 块。返回是否真的写进去了。
fn prefix_content(content: &mut Option<serde_json::Value>, nonce: &str) -> bool {
    let Some(value) = content.as_mut() else { return false };
    match value {
        serde_json::Value::String(text) => {
            *text = format!("{nonce}\n{text}");
            true
        }
        serde_json::Value::Array(blocks) => {
            for block in blocks.iter_mut() {
                if block.get("type").and_then(|t| t.as_str()) != Some("text") {
                    continue;
                }
                let Some(text) = block.get("text").and_then(|t| t.as_str()).map(str::to_string) else {
                    continue;
                };
                block["text"] = serde_json::Value::String(format!("{nonce}\n{text}"));
                return true;
            }
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::UnifiedRequest;

    fn request(system: Option<&str>, messages: serde_json::Value) -> UnifiedRequest {
        let raw = serde_json::json!({
            "model": "m",
            "messages": messages,
            "system": system,
        });
        serde_json::from_value(raw).expect("构造 UnifiedRequest")
    }

    fn first_text(msg: &UnifiedMessage) -> String {
        match msg.content.as_ref() {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Array(blocks)) => blocks
                .iter()
                .find_map(|b| b.get("text").and_then(|t| t.as_str()))
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        }
    }

    #[test]
    fn nonce_is_32_to_128_alphanumeric() {
        for _ in 0..1000 {
            let n = random_nonce();
            assert!((32..=128).contains(&n.len()), "长度越界: {}", n.len());
            assert!(n.chars().all(|c| c.is_ascii_alphanumeric()), "含非法字符: {n}");
            assert!(n.chars().all(|c| c.is_ascii()), "非 ASCII: {n}");
        }
    }

    #[test]
    fn nonce_varies_between_calls() {
        let a = random_nonce();
        let b = random_nonce();
        assert_ne!(a, b, "连续两次生成相同，随机性可疑");
    }

    #[test]
    fn system_field_prefixes_existing_system() {
        let mut u = request(Some("你是严谨的助手"), serde_json::json!([{"role": "user", "content": "hi"}]));
        apply(&mut u, Surface::SystemField);
        let system = u.system.clone().unwrap();
        let (nonce, rest) = system.split_once('\n').expect("应有换行分隔");
        assert!((32..=128).contains(&nonce.len()) && nonce.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(rest, "你是严谨的助手");
        assert_eq!(u.messages.len(), 1, "有 system 时不应插消息");
    }

    #[test]
    fn system_field_without_system_inserts_user_message() {
        let mut u = request(None, serde_json::json!([{"role": "user", "content": "hi"}]));
        apply(&mut u, Surface::SystemField);
        assert_eq!(u.messages.len(), 2);
        assert_eq!(u.messages[0].role, Role::User);
        let nonce = first_text(&u.messages[0]);
        assert!((32..=128).contains(&nonce.len()));
        assert_eq!(first_text(&u.messages[1]), "hi");
    }

    #[test]
    fn messages_prefixes_leading_system_message() {
        let mut u = request(Some("sys"), serde_json::json!([
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "hi"}
        ]));
        apply(&mut u, Surface::Messages);
        let text = first_text(&u.messages[0]);
        let (nonce, rest) = text.split_once('\n').expect("应有换行分隔");
        assert!((32..=128).contains(&nonce.len()));
        assert_eq!(rest, "sys");
        assert_eq!(u.messages.len(), 2, "不应插消息");
    }

    #[test]
    fn messages_prefixes_first_text_block() {
        let mut u = request(None, serde_json::json!([{
            "role": "system",
            "content": [{"type": "text", "text": "块文本"}, {"type": "image_url", "image_url": {"url": "x"}}]
        }]));
        apply(&mut u, Surface::Messages);
        let blocks = u.messages[0].content.as_ref().unwrap().as_array().unwrap();
        let text = blocks[0]["text"].as_str().unwrap();
        assert!(text.ends_with("\n块文本"));
        assert!(blocks[0]["text"].as_str().unwrap().split_once('\n').unwrap().0.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(blocks[1]["type"], "image_url", "非文本块不应被动");
    }

    #[test]
    fn messages_surface_prefixes_system_field_when_messages_lack_system() {
        // Anthropic 入口：system 只在顶层字段，messages 里没有 system 消息
        let mut u = request(Some("你是严谨的助手"), serde_json::json!([{"role": "user", "content": "hi"}]));
        apply(&mut u, Surface::Messages);
        let system = u.system.clone().unwrap();
        let (nonce, rest) = system.split_once('\n').expect("应有换行分隔");
        assert!((32..=128).contains(&nonce.len()));
        assert_eq!(rest, "你是严谨的助手");
        assert_eq!(u.messages.len(), 1, "不应插消息");
    }

    #[test]
    fn messages_without_system_inserts_user_message() {
        let mut u = request(None, serde_json::json!([{"role": "user", "content": "hi"}]));
        apply(&mut u, Surface::Messages);
        assert_eq!(u.messages.len(), 2);
        assert_eq!(u.messages[0].role, Role::User);
        assert_eq!(first_text(&u.messages[1]), "hi");
    }

    #[test]
    fn tools_and_tool_calls_survive_injection() {
        let mut u = request(Some("sys"), serde_json::json!([
            {"role": "user", "content": "查一下"},
            {"role": "assistant", "tool_calls": [{"id": "call_1", "type": "function",
                "function": {"name": "lookup", "arguments": "{\"q\":1}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "结果"}
        ]));
        u.tools = Some(vec![]);
        apply(&mut u, Surface::Messages);
        // 只改 system/首条文本，工具消息的配对不能动
        let with_calls = u.messages.iter().find(|m| m.tool_calls.is_some()).expect("tool_calls 应保留");
        assert_eq!(with_calls.tool_calls.as_ref().unwrap()[0].id, "call_1");
        let tool_msg = u.messages.iter().find(|m| m.role == Role::Tool).expect("tool 消息应保留");
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_1"));
    }
}
