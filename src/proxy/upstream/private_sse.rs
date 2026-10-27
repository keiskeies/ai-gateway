//! 私有协议 SSE 归一化：Trae / WorkBuddy 的流式事件 → 标准 OpenAI chat.completion.chunk。
//!
//! 为什么需要它：私有协议上游**总是回 SSE**（即使客户端要非流式），而且事件形态与
//! OpenAI 不完全一致。归一化成 OpenAI 形态之后，就能复用现有的
//! `StreamConverter`（OpenAI → Anthropic）与透传逻辑，不必为每个上游再写一套转换器。
//!
//! 两个必须修正的点：
//! 1. **`finish_reason` 改写**：上游发 tool_calls 时 `finish_reason` 仍是 `"stop"`，
//!    客户端据此不会执行工具。必须改成 `"tool_calls"`。
//! 2. **非流式客户端**：客户端要 JSON，但上游只给 SSE。归一化后由
//!    `AggregatedStream` 在流结束时拼成一个完整响应。

use serde_json::{json, Value};

/// 上游事件归一化后的结果。
pub enum NormalizedEvent {
    /// 一个 OpenAI 形态的 chunk（可直接序列化成 `data: {...}\n\n`）
    Chunk(Value),
    /// 流内携带的业务错误（Trae 把错误放在 200 + SSE 里）
    BusinessError(crate::proxy::upstream::UpstreamBusinessError),
    /// 无需转发（心跳、未知事件等）
    Ignore,
}

/// 归一化器：按行喂入上游 SSE，吐出 OpenAI 形态事件。
pub struct PrivateSseNormalizer {
    kind: ProtocolKind,
    /// 输入缓冲：SSE 按行解析，避免把半截事件当成完整事件
    line_buf: String,
    /// 已经发出的 role chunk（OpenAI 流要求第一个 chunk 带 role）
    started: bool,
    /// 累积的 tool_calls：上游可能分多帧给，需要按 index 聚合
    tool_calls: Vec<Value>,
    /// 是否已经出现过 tool_call（决定 finish_reason 要不要改写）
    saw_tool_call: bool,
    /// 累积的 usage
    pub usage: Option<Value>,
    /// 累积的正文（非流式聚合用）
    pub text: String,
    /// 累积的推理内容
    pub reasoning: String,
    /// 上游给的 finish_reason
    pub finish_reason: Option<String>,
    pub model: String,
    /// 流内出现过的业务错误。**必须在聚合时暴露出来**——
    /// 上游把错误放在 200 + SSE 里（Trae 4008、WorkBuddy 14018 都是这样），
    /// 若只把它当成一个普通事件吞掉，客户端会收到一个「成功但内容为空」的响应，
    /// 完全看不出是额度耗尽。
    pub business_error: Option<crate::proxy::upstream::UpstreamBusinessError>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtocolKind {
    /// Trae：事件形态与 OpenAI 差异较大（`output` 事件里带 reasoning_content）
    Trae,
    /// WorkBuddy：事件形态接近 OpenAI，主要是 finish_reason 需要修正
    WorkBuddy,
}

impl PrivateSseNormalizer {
    pub fn new(kind: ProtocolKind, model: &str) -> Self {
        Self {
            kind,
            line_buf: String::new(),
            started: false,
            tool_calls: Vec::new(),
            saw_tool_call: false,
            usage: None,
            text: String::new(),
            reasoning: String::new(),
            finish_reason: None,
            model: model.to_string(),
            business_error: None,
        }
    }

    /// 喂入一批字节，返回需要转发给客户端的事件序列。
    pub fn push(&mut self, bytes: &[u8]) -> Vec<NormalizedEvent> {
        self.line_buf.push_str(&String::from_utf8_lossy(bytes));
        let mut out = Vec::new();
        while let Some(nl) = self.line_buf.find('\n') {
            let line: String = self.line_buf.drain(..=nl).collect();
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(event) = self.process_line(line) {
                out.push(event);
            }
        }
        out
    }

    /// 流结束：冲掉残留半行并发出收尾 chunk。
    pub fn finish(&mut self) -> Vec<NormalizedEvent> {
        let mut out = Vec::new();
        let rest = self.line_buf.trim().to_string();
        self.line_buf.clear();
        if !rest.is_empty() {
            if let Some(event) = self.process_line(&rest) {
                out.push(event);
            }
        }
        // 收尾：带 finish_reason 的 chunk（若上游没给就补一个）
        let reason = self.effective_finish_reason();
        out.push(NormalizedEvent::Chunk(self.make_chunk(
            json!(null),
            json!(null),
            Some(json!(reason)),
            self.usage.clone(),
        )));
        out
    }

    /// 有效 finish_reason：出现过 tool_call 就改写为 tool_calls。
    ///
    /// 这是上游的一个真实缺陷：它发 tool_calls 时仍报 `"stop"`，
    /// 客户端看到 `stop` 就不会执行工具，表现为「模型说了要调用工具但没调用」。
    fn effective_finish_reason(&self) -> String {
        if self.saw_tool_call {
            return "tool_calls".to_string();
        }
        match self.finish_reason.as_deref() {
            Some("length") => "length".to_string(),
            Some("content_filter") => "content_filter".to_string(),
            // 上游的 stop / end_turn / null 都归一成 stop
            _ => "stop".to_string(),
        }
    }

    fn process_line(&mut self, line: &str) -> Option<NormalizedEvent> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        // SSE 注释 / 心跳
        if line.starts_with(':') {
            return None;
        }
        let payload = match line.strip_prefix("data:") {
            Some(rest) => rest.trim(),
            // 没有 data: 前缀的行（event: / id: / retry:）忽略
            None => return None,
        };
        if payload == "[DONE]" {
            return None;
        }
        let value: Value = serde_json::from_str(payload).ok()?;

        // 先查业务错误：Trae / WorkBuddy 都可能把错误塞在 200 的 SSE 里
        if let Some(err) = self.detect_business_error(&value, payload) {
            // 记下来：聚合路径要靠它把「空成功响应」变成真正的错误
            self.business_error = Some(err.clone());
            return Some(NormalizedEvent::BusinessError(err));
        }

        match self.kind {
            ProtocolKind::Trae => self.normalize_trae(value),
            ProtocolKind::WorkBuddy => self.normalize_workbuddy(value),
        }
    }

    /// 业务错误检测。两条协议的错误码位置不同，但都是 `code != 0`。
    fn detect_business_error(
        &self,
        value: &Value,
        raw: &str,
    ) -> Option<crate::proxy::upstream::UpstreamBusinessError> {
        let code = value.get("code").and_then(|v| v.as_i64())?;
        if code == 0 {
            return None;
        }
        let message = value
            .get("message")
            .or_else(|| value.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Some(crate::proxy::upstream::UpstreamBusinessError::new(
            Some(code),
            message,
            raw,
        ))
    }

    /// WorkBuddy：事件形态接近 OpenAI，主要是把增量字段摆正 + 记录 tool_calls。
    fn normalize_workbuddy(&mut self, value: Value) -> Option<NormalizedEvent> {
        if let Some(usage) = value.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(usage.clone());
        }
        let choice = value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .cloned();
        let Some(choice) = choice else {
            // 只有 usage 的尾帧：不转发，留到收尾一起给
            return None;
        };
        let delta = choice.get("delta").cloned().unwrap_or_else(|| json!({}));

        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
            self.text.push_str(text);
        }
        if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
            self.reasoning.push_str(reasoning);
        }
        if let Some(calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            self.saw_tool_call = true;
            self.merge_tool_calls(calls);
        }
        if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            if reason != "stop" || self.finish_reason.is_none() {
                self.finish_reason = Some(reason.to_string());
            }
        }

        Some(NormalizedEvent::Chunk(self.make_chunk(
            delta.get("content").cloned().unwrap_or(json!(null)),
            delta.get("reasoning_content").cloned().unwrap_or(json!(null)),
            None,
            None,
        )))
    }

    /// Trae：`output` 事件里带正文与 reasoning，形态与 OpenAI 不同，要重新拼。
    fn normalize_trae(&mut self, value: Value) -> Option<NormalizedEvent> {
        // Trae 的用量字段：有的版本叫 usage，有的嵌在 output 里
        if let Some(usage) = value.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(usage.clone());
        }

        // 事件类型：output / message / delta 都承载增量
        let kind = value
            .get("event")
            .or_else(|| value.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("output");

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<Value> = Vec::new();

        // 正文可能在 output.content / delta.content / message.content
        for path in [
            &["output", "content"][..],
            &["delta", "content"][..],
            &["message", "content"][..],
            &["content"][..],
        ] {
            if let Some(text) = dig_str(&value, path) {
                content.push_str(text);
                break;
            }
        }
        // reasoning_content 在 output.reasoning_content 或 delta.reasoning_content
        for path in [
            &["output", "reasoning_content"][..],
            &["delta", "reasoning_content"][..],
            &["reasoning_content"][..],
        ] {
            if let Some(text) = dig_str(&value, path) {
                reasoning.push_str(text);
                break;
            }
        }
        // tool_calls
        for path in [
            &["output", "tool_calls"][..],
            &["delta", "tool_calls"][..],
            &["tool_calls"][..],
        ] {
            if let Some(calls) = dig_array(&value, path) {
                tool_calls = calls.clone();
                break;
            }
        }
        // 结束原因
        if let Some(reason) = value
            .get("finish_reason")
            .or_else(|| value.get("stop_reason"))
            .and_then(|v| v.as_str())
        {
            self.finish_reason = Some(reason.to_string());
        }
        if kind == "done" || kind == "finish" || kind == "message_stop" {
            // 收尾事件由 finish() 统一补
            return None;
        }

        if content.is_empty() && reasoning.is_empty() && tool_calls.is_empty() {
            return None;
        }
        self.text.push_str(&content);
        self.reasoning.push_str(&reasoning);
        if !tool_calls.is_empty() {
            self.saw_tool_call = true;
            self.merge_tool_calls(&tool_calls);
        }

        Some(NormalizedEvent::Chunk(self.make_chunk(
            if content.is_empty() { json!(null) } else { json!(content) },
            if reasoning.is_empty() { json!(null) } else { json!(reasoning) },
            None,
            None,
        )))
    }

    /// 按 index 合并 tool_calls。上游可能把同一个调用的 name 与 arguments 分帧发。
    fn merge_tool_calls(&mut self, calls: &[Value]) {
        for call in calls {
            let index = call
                .get("index")
                .and_then(|v| v.as_i64())
                .unwrap_or(self.tool_calls.len() as i64);
            let slot = index.max(0) as usize;
            while self.tool_calls.len() <= slot {
                self.tool_calls.push(json!({
                    "index": self.tool_calls.len(),
                    "id": Value::Null,
                    "type": "function",
                    "function": { "name": Value::Null, "arguments": "" }
                }));
            }
            let entry = &mut self.tool_calls[slot];
            if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                entry["id"] = json!(id);
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
            {
                entry["function"]["name"] = json!(name);
            }
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
            {
                let existing = entry["function"]["arguments"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                entry["function"]["arguments"] = json!(format!("{existing}{args}"));
            }
        }
    }

    /// 构造一个 OpenAI 形态的 chunk。
    fn make_chunk(
        &mut self,
        content: Value,
        reasoning: Value,
        finish_reason: Option<Value>,
        usage: Option<Value>,
    ) -> Value {
        let mut delta = json!({});
        // 首个 chunk 必须带 role，否则部分客户端不认
        if !self.started {
            delta["role"] = json!("assistant");
            self.started = true;
        }
        if !content.is_null() {
            delta["content"] = content;
        }
        if !reasoning.is_null() {
            delta["reasoning_content"] = reasoning;
        }
        // tool_calls 只在收尾时整批给（已合并完整），避免分帧导致的残缺
        if finish_reason.is_some() && !self.tool_calls.is_empty() {
            delta["tool_calls"] = json!(self.tool_calls);
        }

        let mut chunk = json!({
            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
            "object": "chat.completion.chunk",
            "model": self.model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason.unwrap_or(json!(null))
            }]
        });
        if let Some(usage) = usage {
            chunk["usage"] = usage;
        }
        chunk
    }

    /// 把累积的内容拼成一个完整的非流式响应（客户端要 JSON 但上游只给 SSE 时用）。
    pub fn into_response(&self) -> Value {
        let mut message = json!({
            "role": "assistant",
            "content": if self.text.is_empty() { Value::Null } else { json!(self.text) },
        });
        if !self.reasoning.is_empty() {
            message["reasoning_content"] = json!(self.reasoning);
        }
        if !self.tool_calls.is_empty() {
            // 去掉 index 字段（非流式响应里不需要）
            let cleaned: Vec<Value> = self
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.get("id").cloned().unwrap_or(json!(format!("call_{}", uuid::Uuid::new_v4().simple()))),
                        "type": "function",
                        "function": {
                            "name": call["function"]["name"].clone(),
                            "arguments": call["function"]["arguments"].clone()
                        }
                    })
                })
                .collect();
            message["tool_calls"] = json!(cleaned);
        }
        // `created` 是 OpenAI 响应的必填字段：缺了它，网关自己的 parse_response
        // 会报 "missing field `created`"，跨协议（→ Anthropic）转换会整条失败。
        let mut response = json!({
            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": self.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": self.effective_finish_reason()
            }]
        });
        if let Some(usage) = &self.usage {
            response["usage"] = usage.clone();
        }
        response
    }
}

/// 按路径取字符串。
fn dig_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

/// 按路径取数组。
fn dig_array<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(normalizer: &mut PrivateSseNormalizer, input: &str) -> Vec<NormalizedEvent> {
        normalizer.push(input.as_bytes())
    }

    #[test]
    fn workbuddy_delta_becomes_openai_chunk() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "glm-5.3");
        let events = collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"},\"finish_reason\":null}]}\n\n",
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            NormalizedEvent::Chunk(chunk) => {
                assert_eq!(chunk["object"], "chat.completion.chunk");
                assert_eq!(chunk["choices"][0]["delta"]["content"], "你好");
                // 首个 chunk 必须带 role
                assert_eq!(chunk["choices"][0]["delta"]["role"], "assistant");
            }
            _ => panic!("应该是 chunk"),
        }
        assert_eq!(n.text, "你好");
    }

    #[test]
    fn finish_reason_is_rewritten_to_tool_calls_when_tools_present() {
        // 上游的真实缺陷：发 tool_calls 时 finish_reason 仍是 "stop"
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f1\",\"arguments\":\"{\\\"a\\\":1}\"}}]},\"finish_reason\":\"stop\"}]}\n\n",
        );
        let tail = n.finish();
        match tail.last().unwrap() {
            NormalizedEvent::Chunk(chunk) => {
                assert_eq!(
                    chunk["choices"][0]["finish_reason"], "tool_calls",
                    "有 tool_calls 时必须改写成 tool_calls，否则客户端不执行工具"
                );
            }
            _ => panic!("应该是 chunk"),
        }
    }

    #[test]
    fn plain_text_keeps_stop_finish_reason() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n",
        );
        let tail = n.finish();
        match tail.last().unwrap() {
            NormalizedEvent::Chunk(chunk) => {
                assert_eq!(chunk["choices"][0]["finish_reason"], "stop");
            }
            _ => panic!("应该是 chunk"),
        }
    }

    #[test]
    fn length_finish_reason_is_preserved() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"content\":\"x\"},\"finish_reason\":\"length\"}]}\n\n",
        );
        assert_eq!(n.finish_reason.as_deref(), Some("length"));
        let tail = n.finish();
        match tail.last().unwrap() {
            NormalizedEvent::Chunk(chunk) => assert_eq!(chunk["choices"][0]["finish_reason"], "length"),
            _ => panic!(),
        }
    }

    #[test]
    fn tool_calls_are_merged_across_frames() {
        // 上游可能把 name 与 arguments 分帧发，必须按 index 合并
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"f1\",\"arguments\":\"{\\\"a\\\"\"}}]}}]}\n\n",
        );
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":1}\"}}]}}]}\n\n",
        );
        let response = n.into_response();
        assert_eq!(response["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "f1");
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"a\":1}"
        );
    }

    #[test]
    fn business_error_is_surfaced() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        let events = collect(&mut n, "data: {\"code\":14018,\"msg\":\"credits exhausted\"}\n\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            NormalizedEvent::BusinessError(err) => {
                assert_eq!(err.code, Some(14018));
                assert_eq!(err.message, "credits exhausted");
            }
            _ => panic!("应该识别为业务错误"),
        }
    }

    #[test]
    fn trae_output_event_is_normalized() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::Trae, "glm-5.2");
        let events = collect(
            &mut n,
            "data: {\"event\":\"output\",\"output\":{\"content\":\"答案\",\"reasoning_content\":\"思考\"}}\n\n",
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            NormalizedEvent::Chunk(chunk) => {
                assert_eq!(chunk["choices"][0]["delta"]["content"], "答案");
                assert_eq!(chunk["choices"][0]["delta"]["reasoning_content"], "思考");
            }
            _ => panic!(),
        }
        assert_eq!(n.text, "答案");
        assert_eq!(n.reasoning, "思考");
    }

    #[test]
    fn trae_done_event_is_swallowed() {
        // 收尾事件由 finish() 统一补，不该转发成空 chunk
        let mut n = PrivateSseNormalizer::new(ProtocolKind::Trae, "m");
        let events = collect(&mut n, "data: {\"event\":\"done\"}\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn heartbeat_and_done_marker_are_ignored() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        assert!(collect(&mut n, ": ping\n\n").is_empty());
        assert!(collect(&mut n, "data: [DONE]\n\n").is_empty());
    }

    #[test]
    fn usage_frame_is_captured_not_forwarded() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        let events = collect(
            &mut n,
            "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5},\"choices\":[]}\n\n",
        );
        assert!(events.is_empty(), "纯 usage 帧不转发");
        let response = n.into_response();
        assert_eq!(response["usage"]["prompt_tokens"], 10);
    }

    #[test]
    fn into_response_has_assistant_role_and_content() {
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "glm-5.3");
        collect(&mut n, "data: {\"choices\":[{\"delta\":{\"content\":\"完整回答\"}}]}\n\n");
        let response = n.into_response();
        assert_eq!(response["object"], "chat.completion");
        assert_eq!(response["choices"][0]["message"]["role"], "assistant");
        assert_eq!(response["choices"][0]["message"]["content"], "完整回答");
        assert_eq!(response["model"], "glm-5.3");
    }

    #[test]
    fn into_response_includes_created_for_cross_protocol_conversion() {
        // 回归：缺 `created` 会让网关自己的 openai::parse_response 报
        // "missing field `created`"，进而导致 /v1/messages 入口拿不到 Anthropic 形态，
        // 静默退回 OpenAI 形态（Anthropic 客户端解析失败）。
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(&mut n, "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
        let response = n.into_response();
        assert!(
            response.get("created").and_then(|v| v.as_i64()).is_some(),
            "created 必须是数字，实际：{:?}",
            response.get("created")
        );
        // 直接验证能被网关的 OpenAI 解析器接受（这才是跨协议转换的前提）
        assert!(
            crate::protocol::openai::parse_response(response).is_ok(),
            "聚合出的响应必须能被 openai::parse_response 解析"
        );
    }

    #[test]
    fn into_response_is_parseable_by_gateway_openai_parser() {
        // 带 tool_calls 的形态也要能被解析器接受
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        collect(
            &mut n,
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"f1\",\"arguments\":\"{}\"}}]}}]}\n\n",
        );
        let response = n.into_response();
        let parsed = crate::protocol::openai::parse_response(response);
        assert!(parsed.is_ok(), "带 tool_calls 的响应也必须可解析：{:?}", parsed.err());
    }

    #[test]
    fn partial_line_is_buffered_until_newline() {
        // 分帧到达时必须等换行再解析，否则会把半截 JSON 当完整事件
        let mut n = PrivateSseNormalizer::new(ProtocolKind::WorkBuddy, "m");
        assert!(collect(&mut n, "data: {\"choices\":[{\"delta\"").is_empty());
        let events = collect(&mut n, ":{\"content\":\"hi\"}}]}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(n.text, "hi");
    }
}
