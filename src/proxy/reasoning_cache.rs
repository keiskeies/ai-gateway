//! 跨协议工具循环的 Anthropic thinking 签名缓存。
//!
//! OpenAI 客户端不会回传 Anthropic 的 `signature`。当上游是 Anthropic 且响应包含
//! `tool_use` 时，把 thinking / redacted_thinking 块按 tool id 缓存；下一轮请求里
//! 客户端带回 tool_calls 时再注入，避免上游因缺少 thinking 块拒绝请求。
//!
//! 仅内存保存，不落盘；只在 debug 级别记录命中/未命中，不记录思考内容。
//! 缓存未命中或 backend/model 不一致时由调用方 fail-open（关闭本次 thinking）。

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(30 * 60);
const MAX_ENTRIES: usize = 4096;

#[derive(Clone)]
struct CacheEntry {
    blocks: Arc<Vec<Value>>,
    backend_id: String,
    model_id: String,
    inserted_at: Instant,
}

static CACHE: Lazy<DashMap<String, CacheEntry>> = Lazy::new(DashMap::new);

/// 缓存一组 thinking 块，供同一响应中的所有 tool_use id 查询。
pub fn store(tool_ids: &[String], blocks: &[Value], backend_id: &str, model_id: &str) {
    if tool_ids.is_empty() || blocks.is_empty() {
        return;
    }
    evict_if_needed();
    let blocks = Arc::new(blocks.to_vec());
    tracing::debug!("缓存思考签名：{} 个 thinking 块 / {} 个 tool id", blocks.len(), tool_ids.len());
    let now = Instant::now();
    for id in tool_ids {
        if id.is_empty() {
            continue;
        }
        CACHE.insert(
            id.clone(),
            CacheEntry {
                blocks: blocks.clone(),
                backend_id: backend_id.to_string(),
                model_id: model_id.to_string(),
                inserted_at: now,
            },
        );
    }
}

/// 查询 tool id 对应的 thinking 块；backend/model 不一致或已过期时返回 None。
pub fn lookup(tool_id: &str, backend_id: &str, model_id: &str) -> Option<Vec<Value>> {
    if tool_id.is_empty() {
        return None;
    }
    let Some(entry) = CACHE.get(tool_id) else {
        tracing::debug!("思考签名缓存未命中：tool_id={}（fail-open 关闭 thinking）", tool_id);
        return None;
    };
    if entry.inserted_at.elapsed() > TTL {
        drop(entry);
        CACHE.remove(tool_id);
        tracing::debug!("思考签名缓存已过期：tool_id={}", tool_id);
        return None;
    }
    if entry.backend_id != backend_id || entry.model_id != model_id {
        tracing::debug!(
            "思考签名缓存归属不匹配：tool_id={} cached={}/{} current={}/{}",
            tool_id,
            entry.backend_id,
            entry.model_id,
            backend_id,
            model_id
        );
        return None;
    }
    Some(entry.blocks.as_ref().clone())
}

/// 从统一响应的 assistant 消息里提取 thinking 块并按 tool id 缓存。
pub fn cache_assistant_message(
    msg: &crate::protocol::types::UnifiedMessage,
    backend_id: &str,
    model_id: &str,
) {
    let Some(tool_calls) = msg.tool_calls.as_ref().filter(|t| !t.is_empty()) else {
        return;
    };
    if msg.reasoning_blocks.is_empty() {
        return;
    }
    let ids: Vec<String> = tool_calls.iter().map(|t| t.id.clone()).collect();
    store(&ids, &msg.reasoning_blocks, backend_id, model_id);
}

fn evict_if_needed() {
    if CACHE.len() < MAX_ENTRIES {
        return;
    }
    let now = Instant::now();
    CACHE.retain(|_, entry| now.duration_since(entry.inserted_at) <= TTL);
    while CACHE.len() >= MAX_ENTRIES {
        let oldest = CACHE
            .iter()
            .min_by_key(|entry| entry.value().inserted_at)
            .map(|entry| entry.key().clone());
        match oldest {
            Some(key) => {
                CACHE.remove(&key);
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn store_and_lookup_with_binding() {
        let blocks = vec![json!({"type": "thinking", "thinking": "x", "signature": "sig"})];
        let id = format!("toolu_test_{}", uuid::Uuid::new_v4().simple());
        store(&[id.clone()], &blocks, "b1", "m1");
        assert_eq!(lookup(&id, "b1", "m1").unwrap().len(), 1);
        assert!(lookup(&id, "b2", "m1").is_none());
        assert!(lookup("missing", "b1", "m1").is_none());
    }
}
