//! 跨协议流式转换器：上游 SSE 事件流 → 客户端协议事件流（逐事件实时转换）
//!
//! - Anthropic 上游 → OpenAI 客户端（/v1/chat/completions 入口 + anthropic 平台 backend）
//! - OpenAI 上游 → Anthropic 客户端（/v1/messages 入口 + openai 兼容平台 backend）
//!
//! 同协议流式不走本模块（handler 直接透传）。

use serde_json::{json, Value};
use std::collections::HashMap;

pub struct StreamConverter {
    /// true = 客户端是 Anthropic 协议（/v1/messages），输出 Anthropic 事件流
    client_anthropic: bool,
    /// 对外展示的模型名（虚拟模型名）
    model: String,

    // ---- SSE 输入缓冲：按空行分割事件，event:/data: 行聚合 ----
    line_buf: String,
    cur_event: String,
    cur_data: String,

    // ---- 输出 ----
    out: String,

    // ---- 方向 1：Anthropic 上游 → OpenAI 客户端 ----
    openai_id: String,
    /// 是否已发出首个 role chunk
    started: bool,
    /// anthropic content block index → openai tool_calls index（已 start 的 tool_use 块）
    tool_block_to_idx: HashMap<i64, i64>,
    tool_counter: i64,

    // ---- 方向 2：OpenAI 上游 → Anthropic 客户端 ----
    msg_id: String,
    /// 下一个 content block 的 index
    next_block: usize,
    /// 已 start 未 stop 的 block index（按 start 顺序）
    open_blocks: Vec<usize>,
    /// openai tool_calls index → anthropic block index
    tool_idx_to_block: HashMap<i64, usize>,
    /// 当前 text 块（连续 text_delta 复用同一块）
    text_block: Option<usize>,
    /// 当前 thinking 块
    thinking_block: Option<usize>,
    /// finish_reason 映射结果（收到时暂存，收尾统一发）
    stop_reason: Option<&'static str>,

    // ---- usage 统计（从流内事件提取，供 StreamStatCtx 记账）----
    usage_in: i64,
    usage_out: i64,
    usage_cache_read: i64,
    usage_cache_write: i64,
    /// 上游流已正常收尾（message_stop / [DONE] 已处理），finish() 不再补发
    finished: bool,
}

impl StreamConverter {
    pub fn new(client_anthropic: bool, model: &str) -> Self {
        Self {
            client_anthropic,
            model: model.to_string(),
            line_buf: String::new(),
            cur_event: String::new(),
            cur_data: String::new(),
            out: String::new(),
            openai_id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
            started: false,
            tool_block_to_idx: HashMap::new(),
            tool_counter: 0,
            msg_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            next_block: 0,
            open_blocks: Vec::new(),
            tool_idx_to_block: HashMap::new(),
            text_block: None,
            thinking_block: None,
            stop_reason: None,
            usage_in: 0,
            usage_out: 0,
            usage_cache_read: 0,
            usage_cache_write: 0,
            finished: false,
        }
    }

    /// 上游到达一批字节，返回应转发给客户端的字节（可能为空）
    pub fn push_chunk(&mut self, bytes: &[u8]) -> String {
        self.out.clear();
        self.line_buf.push_str(&String::from_utf8_lossy(bytes));
        while let Some(nl) = self.line_buf.find('\n') {
            let line: String = self.line_buf.drain(..=nl).collect();
            self.process_line(line.trim_end_matches(['\r', '\n']));
        }
        self.out.clone()
    }

    /// 上游流结束：补发收尾事件（若上游未正常收尾）并返回剩余输出
    pub fn finish(&mut self) -> String {
        self.out.clear();
        // 残留半行：尽力解析一次
        let rest = self.line_buf.trim().to_string();
        self.line_buf.clear();
        if !rest.is_empty() {
            if let Some(data) = rest.strip_prefix("data:") {
                self.cur_data.push_str(data.trim());
                self.dispatch_current();
            }
        }
        if !self.finished {
            if self.client_anthropic {
                self.finish_anthropic();
            } else {
                // OpenAI 客户端：补 finish + usage + [DONE]
                let fr = self.stop_reason.unwrap_or("stop");
                self.emit_openai_chunk(json!({}), Some(fr), true);
                self.out.push_str("data: [DONE]\n\n");
            }
            self.finished = true;
        }
        self.out.clone()
    }

    /// 上游流出错：按客户端协议发错误帧并正常收尾（SSE 已开始，无法改 HTTP 状态码）
    pub fn emit_error(&mut self, message: &str) -> String {
        self.out.clear();
        if self.client_anthropic {
            self.emit_anthropic_event("error", &json!({
                "type": "error",
                "error": {"type": "api_error", "message": message}
            }));
        } else {
            self.emit_data(&json!({"error": {"message": message, "type": "upstream_error"}}));
        }
        if !self.finished {
            if self.client_anthropic {
                self.finish_anthropic();
            } else {
                self.out.push_str("data: [DONE]\n\n");
            }
            self.finished = true;
        }
        self.out.clone()
    }

    pub fn usage(&self) -> (i64, i64, i64, i64) {
        (self.usage_in, self.usage_out, self.usage_cache_read, self.usage_cache_write)
    }

    // ================= 内部：SSE 解析 =================

    fn process_line(&mut self, line: &str) {
        if line.is_empty() {
            // 空行 = 事件边界
            self.dispatch_current();
            return;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            self.cur_event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !self.cur_data.is_empty() {
                self.cur_data.push('\n');
            }
            self.cur_data.push_str(rest.trim());
        }
        // 注释行（: 开头）与其他字段忽略
    }

    fn dispatch_current(&mut self) {
        let data = std::mem::take(&mut self.cur_data);
        let event = std::mem::take(&mut self.cur_event);
        if data.is_empty() { return; }
        if data == "[DONE]" {
            // OpenAI 上游收尾
            if self.client_anthropic {
                self.finish_anthropic();
                self.finished = true;
            }
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(&data) else { return };
        // Anthropic 事件以 data.type 分发（event: 名仅作参考）
        if self.client_anthropic {
            self.openai_upstream_event(v);
        } else {
            self.anthropic_upstream_event(v);
        }
        let _ = event;
    }

    // ================= 方向 1：Anthropic 上游 → OpenAI 客户端 =================

    fn anthropic_upstream_event(&mut self, v: Value) {
        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match typ {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    let cr = u.get("cache_read_input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
                    let cw = u.get("cache_creation_input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
                    self.usage_cache_read = cr;
                    self.usage_cache_write = cw;
                    // Anthropic input_tokens 不含缓存部分，统一为"总提示词"口径
                    self.usage_in = u.get("input_tokens").and_then(|x| x.as_i64()).unwrap_or(0) + cr + cw;
                }
                // 首个 chunk 携带 role
                self.emit_openai_chunk(json!({"role": "assistant", "content": ""}), None, false);
                self.started = true;
            }
            "content_block_start" => {
                let index = v.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                let block = v.get("content_block").cloned().unwrap_or(json!({}));
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = block.get("id").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let name = block.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let tc_index = self.tool_counter;
                    self.tool_counter += 1;
                    self.tool_block_to_idx.insert(index, tc_index);
                    self.emit_openai_chunk(json!({
                        "tool_calls": [{
                            "index": tc_index,
                            "id": id,
                            "type": "function",
                            "function": {"name": name, "arguments": ""}
                        }]
                    }), None, false);
                }
                // text / thinking 块无需发 chunk，等 delta
            }
            "content_block_delta" => {
                let index = v.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                let delta = v.get("delta").cloned().unwrap_or(json!({}));
                match delta.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text_delta" => {
                        if let Some(t) = delta.get("text").and_then(|t| t.as_str()) {
                            self.emit_openai_chunk(json!({"content": t}), None, false);
                        }
                    }
                    "thinking_delta" => {
                        if let Some(t) = delta.get("thinking").and_then(|t| t.as_str()) {
                            self.emit_openai_chunk(json!({"reasoning_content": t}), None, false);
                        }
                    }
                    "input_json_delta" => {
                        if let Some(&tc_index) = self.tool_block_to_idx.get(&index) {
                            if let Some(p) = delta.get("partial_json").and_then(|t| t.as_str()) {
                                self.emit_openai_chunk(json!({
                                    "tool_calls": [{
                                        "index": tc_index,
                                        "function": {"arguments": p}
                                    }]
                                }), None, false);
                            }
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(sr) = v.pointer("/delta/stop_reason").and_then(|s| s.as_str()) {
                    self.stop_reason = Some(match sr {
                        "max_tokens" => "length",
                        "tool_use" => "tool_calls",
                        "refusal" => "content_filter",
                        _ => "stop", // end_turn / stop_sequence / pause_turn 等
                    });
                }
                if let Some(u) = v.get("usage") {
                    if let Some(o) = u.get("output_tokens").and_then(|x| x.as_i64()) {
                        self.usage_out = o;
                    }
                }
            }
            "message_stop" => {
                let fr = self.stop_reason.unwrap_or("stop");
                self.emit_openai_chunk(json!({}), Some(fr), true);
                self.out.push_str("data: [DONE]\n\n");
                self.finished = true;
            }
            "error" => {
                let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("upstream stream error").to_string();
                let out = self.emit_error(&msg);
                self.out.clear();
                self.out.push_str(&out);
            }
            _ => {} // ping / content_block_stop / 未知事件忽略
        }
    }

    /// 发一个 OpenAI chunk（usage=true 时附带 token 统计并作为终止 chunk）
    fn emit_openai_chunk(&mut self, delta: Value, finish_reason: Option<&str>, with_usage: bool) {
        let mut chunk = json!({
            "id": self.openai_id,
            "object": "chat.completion.chunk",
            "created": chrono::Utc::now().timestamp(),
            "model": self.model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason,
            }]
        });
        if with_usage {
            chunk["usage"] = json!({
                "prompt_tokens": self.usage_in,
                "completion_tokens": self.usage_out,
                "total_tokens": self.usage_in + self.usage_out,
                // 缓存命中信息透传给 OpenAI 客户端（无缓存数据时省略，避免误导）
                "prompt_tokens_details": { "cached_tokens": self.usage_cache_read },
            });
        }
        self.emit_data(&chunk);
    }

    // ================= 方向 2：OpenAI 上游 → Anthropic 客户端 =================

    fn openai_upstream_event(&mut self, v: Value) {
        if let Some(u) = v.get("usage") {
            self.usage_in = u.get("prompt_tokens").and_then(|x| x.as_i64()).unwrap_or(self.usage_in);
            self.usage_out = u.get("completion_tokens").and_then(|x| x.as_i64()).unwrap_or(self.usage_out);
            // 缓存命中：OpenAI 标准 prompt_tokens_details.cached_tokens，兜底 DeepSeek 老格式
            let cr = u.get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens")).and_then(|x| x.as_i64())
                .or_else(|| u.get("prompt_cache_hit_tokens").and_then(|x| x.as_i64()));
            if let Some(cr) = cr {
                self.usage_cache_read = cr;
            }
        }
        // 错误帧（无 choices）：透传给 Anthropic 客户端
        if v.get("error").is_some() {
            let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("upstream stream error").to_string();
            let out = self.emit_error(&msg);
            self.out.clear();
            self.out.push_str(&out);
            return;
        }
        let choice = match v.get("choices").and_then(|c| c.get(0)) {
            Some(c) => c.clone(),
            None => return,
        };
        if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            self.stop_reason = Some(match fr {
                "length" => "max_tokens",
                "tool_calls" | "function_call" => "tool_use",
                _ => "end_turn",
            });
        }
        let delta = choice.get("delta").cloned().unwrap_or(json!({}));

        if !self.started {
            self.started = true;
            self.emit_anthropic_event("message_start", &json!({
                "type": "message_start",
                "message": {
                    "id": self.msg_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": Value::Null,
                    "usage": {"input_tokens": self.usage_in, "output_tokens": 0}
                }
            }));
        }

        // thinking（DeepSeek R 系风格）
        if let Some(t) = delta.get("reasoning_content").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                let idx = match self.thinking_block {
                    Some(i) => i,
                    None => {
                        let i = self.next_block;
                        self.next_block += 1;
                        self.thinking_block = Some(i);
                        self.open_blocks.push(i);
                        self.emit_anthropic_event("content_block_start", &json!({
                            "type": "content_block_start",
                            "index": i,
                            "content_block": {"type": "thinking", "thinking": ""}
                        }));
                        i
                    }
                };
                self.emit_anthropic_event("content_block_delta", &json!({
                    "type": "content_block_delta",
                    "index": idx,
                    "delta": {"type": "thinking_delta", "thinking": t}
                }));
            }
        }

        // 文本
        if let Some(t) = delta.get("content").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                let idx = match self.text_block {
                    Some(i) => i,
                    None => {
                        let i = self.next_block;
                        self.next_block += 1;
                        self.text_block = Some(i);
                        self.open_blocks.push(i);
                        self.emit_anthropic_event("content_block_start", &json!({
                            "type": "content_block_start",
                            "index": i,
                            "content_block": {"type": "text", "text": ""}
                        }));
                        i
                    }
                };
                self.emit_anthropic_event("content_block_delta", &json!({
                    "type": "content_block_delta",
                    "index": idx,
                    "delta": {"type": "text_delta", "text": t}
                }));
            }
        }

        // 工具调用（可多个并行 index）
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let tc_index = tc.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                let block = match self.tool_idx_to_block.get(&tc_index) {
                    Some(&b) => b,
                    None => {
                        let i = self.next_block;
                        self.next_block += 1;
                        self.tool_idx_to_block.insert(tc_index, i);
                        self.open_blocks.push(i);
                        let id = tc.pointer("/id").and_then(|s| s.as_str()).unwrap_or("");
                        let name = tc.pointer("/function/name").and_then(|s| s.as_str()).unwrap_or("");
                        self.emit_anthropic_event("content_block_start", &json!({
                            "type": "content_block_start",
                            "index": i,
                            "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
                        }));
                        i
                    }
                };
                if let Some(args) = tc.pointer("/function/arguments").and_then(|s| s.as_str()) {
                    if !args.is_empty() {
                        self.emit_anthropic_event("content_block_delta", &json!({
                            "type": "content_block_delta",
                            "index": block,
                            "delta": {"type": "input_json_delta", "partial_json": args}
                        }));
                    }
                }
            }
        }
    }

    /// Anthropic 收尾序列：stop 所有未关闭 block → message_delta（stop_reason + output_tokens）→ message_stop
    fn finish_anthropic(&mut self) {
        let blocks = std::mem::take(&mut self.open_blocks);
        for i in blocks {
            self.emit_anthropic_event("content_block_stop", &json!({
                "type": "content_block_stop", "index": i
            }));
        }
        self.text_block = None;
        self.thinking_block = None;
        let sr = self.stop_reason.unwrap_or("end_turn");
        self.emit_anthropic_event("message_delta", &json!({
            "type": "message_delta",
            "delta": {"stop_reason": sr, "stop_sequence": Value::Null},
            "usage": {"output_tokens": self.usage_out}
        }));
        self.emit_anthropic_event("message_stop", &json!({"type": "message_stop"}));
    }

    // ================= 输出辅助 =================

    fn emit_data(&mut self, v: &Value) {
        self.out.push_str("data: ");
        self.out.push_str(&v.to_string());
        self.out.push_str("\n\n");
    }

    fn emit_anthropic_event(&mut self, event: &str, data: &Value) {
        self.out.push_str("event: ");
        self.out.push_str(event);
        self.out.push_str("\ndata: ");
        self.out.push_str(&data.to_string());
        self.out.push_str("\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anthropic 文本流 → OpenAI chunk 流：role chunk、文本增量、finish=stop、usage、[DONE]
    #[test]
    fn anthropic_text_to_openai() {
        let mut c = StreamConverter::new(false, "my-model");
        let mut out = String::new();
        let events = [
            r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","role":"assistant","usage":{"input_tokens":10,"output_tokens":1}}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
            r#"event: message_stop
data: {"type":"message_stop"}"#,
        ];
        for e in events {
            out.push_str(&c.push_chunk(format!("{e}\n\n").as_bytes()));
        }
        out.push_str(&c.finish());
        assert!(out.contains("\"role\": \"assistant\"") || out.contains("\"role\":\"assistant\""));
        assert!(out.contains("Hel") && out.contains("lo"));
        assert!(out.contains("\"finish_reason\":\"stop\""));
        assert!(out.contains("\"prompt_tokens\":10") && out.contains("\"completion_tokens\":5"));
        assert!(out.contains("data: [DONE]"));
        assert_eq!(c.usage(), (10, 5, 0, 0));
    }

    /// Anthropic tool_use 流 → OpenAI tool_calls chunk 流（name 先行、arguments 增量、finish=tool_calls）
    #[test]
    fn anthropic_tool_to_openai() {
        let mut c = StreamConverter::new(false, "my-model");
        let mut out = String::new();
        let events = [
            r#"data: {"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":8,"output_tokens":1}}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_time"}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"tz\""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":":\"UTC\"}"}}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":12}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        for e in events {
            out.push_str(&c.push_chunk(format!("{e}\n\n").as_bytes()));
        }
        out.push_str(&c.finish());
        assert!(out.contains("\"name\":\"get_time\"") || out.contains("\"name\": \"get_time\""));
        assert!(out.contains("\"arguments\":\"{\\\"tz\\\"\"") || out.contains("\"arguments\": \"{\\\"tz\\\"\""));
        assert!(out.contains("\"finish_reason\":\"tool_calls\""));
        assert_eq!(c.usage(), (8, 12, 0, 0));
    }

    /// OpenAI 文本流 → Anthropic 事件流：message_start、text block、message_delta、message_stop
    #[test]
    fn openai_text_to_anthropic() {
        let mut c = StreamConverter::new(true, "my-model");
        let mut out = String::new();
        let chunks = [
            r#"{"id":"c1","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"{"id":"c1","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
            r#"{"id":"c1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":7}}"#,
        ];
        for ch in chunks {
            out.push_str(&c.push_chunk(format!("data: {ch}\n\n").as_bytes()));
        }
        out.push_str(&c.push_chunk(b"data: [DONE]\n\n"));
        out.push_str(&c.finish());
        assert!(out.contains("event: message_start"));
        assert!(out.contains("\"type\":\"text_delta\",\"text\":\"Hi\"") || out.contains("\"text\":\"Hi\""));
        assert!(out.contains("event: content_block_stop"));
        assert!(out.contains("\"stop_reason\":\"end_turn\""));
        assert!(out.contains("\"output_tokens\":7"));
        assert!(out.contains("event: message_stop"));
        assert_eq!(c.usage(), (3, 7, 0, 0));
    }

    /// OpenAI tool_calls 流 → Anthropic tool_use 块（start 带 id/name，增量 arguments）
    #[test]
    fn openai_tool_to_anthropic() {
        let mut c = StreamConverter::new(true, "my-model");
        let mut out = String::new();
        let chunks = [
            r#"{"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_time","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"tz\":"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"UTC\"}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":9}}"#,
        ];
        for ch in chunks {
            out.push_str(&c.push_chunk(format!("data: {ch}\n\n").as_bytes()));
        }
        out.push_str(&c.push_chunk(b"data: [DONE]\n\n"));
        out.push_str(&c.finish());
        assert!(out.contains("\"type\":\"tool_use\""));
        assert!(out.contains("\"name\":\"get_time\""));
        assert!(out.contains("input_json_delta"));
        assert!(out.contains("\"stop_reason\":\"tool_use\""));
        assert!(out.contains("event: message_stop"));
    }

    /// 上游半包：chunk 切在事件中间也能正确解析（跨 chunk 的事件不丢）
    #[test]
    fn split_chunk_boundaries() {
        let mut a = StreamConverter::new(false, "m");
        let mut b = StreamConverter::new(false, "m");
        let full = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":4}}}\n\ndata: {\"type\":\"message_stop\"}\n\n";
        let whole = {
            let mut o = a.push_chunk(full.as_bytes());
            o.push_str(&a.finish());
            o
        };
        let sliced = {
            let mut o = String::new();
            // 按奇怪的字节边界切块
            for part in [full[..30].as_bytes(), full[30..77].as_bytes(), full[77..].as_bytes()] {
                o.push_str(&b.push_chunk(part));
            }
            o.push_str(&b.finish());
            o
        };
        // 归一化：去空白、挖掉随机 id（chatcmpl-<hex>）与 created 时间戳后比较
        let norm = |s: &str| -> String {
            let s: String = s.chars().filter(|c| *c != '\n' && *c != ' ').collect();
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(p) = rest.find("\"chatcmpl-") {
                let cut = p + 10;
                out.push_str(&rest[..cut]);
                rest = &rest[cut..];
                let end = rest.find('"').unwrap_or(0);
                rest = &rest[end..];
            }
            out.push_str(rest);
            let mut out2 = String::new();
            let mut rest = out.as_str();
            while let Some(p) = rest.find("\"created\":") {
                let cut = p + 10;
                out2.push_str(&rest[..cut]);
                rest = &rest[cut..];
                let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
                rest = &rest[digits..];
            }
            out2.push_str(rest);
            out2
        };
        assert_eq!(norm(&whole), norm(&sliced));
        assert_eq!(a.usage(), b.usage());
    }

    /// 上游出错：OpenAI 客户端收到错误帧 + [DONE]，不 panic
    #[test]
    fn upstream_error_frame() {
        let mut c = StreamConverter::new(false, "m");
        let out = c.emit_error("boom");
        assert!(out.contains("\"error\""));
        assert!(out.contains("data: [DONE]"));
    }
}
