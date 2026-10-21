# AI Gateway 思考模式（Reasoning / Thinking）透传优化方案

> 状态：设计稿，待评审后实施  
> 范围：`/v1/chat/completions`（OpenAI 协议）与 `/v1/messages`（Anthropic 协议）  
> 目标版本：待定

---

## 1. 背景与结论

Qoder、Trae 等编码 Agent 的“思考等级”开关来自客户端内置的模型能力表，不是通过 OpenAI/Anthropic 接口协商出来的。自定义模型通常只被当作普通 chat 模型，因此网关无法单方面让这些客户端“长出”思考滑块。

但网关目前还有另一类问题：**即使客户端主动发送了思考参数，网关也会丢弃或无法正确回传**。本方案解决的是这一类问题，目标是：

> 任何愿意发送 `reasoning_effort` / `thinking` / `enable_thinking` 的客户端，经过网关后都能得到正确的参数转发、跨协议映射、思考内容回传，以及带工具调用的多轮对话支持。

对于 Qoder/Trae 是否展示开关，属于客户端限制，不在本方案能解决的范围内；本方案只保证网关侧“不拖后腿”。

---

## 2. 现状审计（代码级）

### 2.1 已确认的问题

| # | 问题 | 位置 | 影响 |
|---|---|---|---|
| 1 | Anthropic 后端的未知参数策略是 `Drop`，而 `thinking` 未建模、只存在于 `extra`，因此被直接丢弃 | `src/proxy/handler.rs:296-304`、`src/protocol/anthropic.rs:27-29`、`src/protocol/anthropic.rs:261-269` | `/v1/messages` → Claude 后端时，客户端传的 extended thinking 永远开不起来 |
| 2 | OpenAI 的 `reasoning_effort` 与 Anthropic 的 `thinking` 没有互转 | `src/protocol/openai.rs:198-203`、`src/protocol/openai.rs:256-263` | OpenAI 客户端请求 Claude 后端时，思考参数被 Drop；Anthropic 客户端请求 OpenAI 后端时，参数形状不匹配 |
| 3 | Anthropic assistant 历史里的 `thinking` / `redacted_thinking` 块只被提取成文本，`signature` 丢失，转发时也不会重建 | `src/protocol/anthropic.rs:133-157`、`src/protocol/anthropic.rs:197-207` | 带工具的后续轮次缺少 thinking 块，Anthropic 会拒绝；思考链路断裂 |
| 4 | 非流式 Anthropic 响应的 `content` 枚举没有 `thinking` / `redacted_thinking` 变体，解析会失败 | `src/protocol/anthropic.rs:58-67`、`src/protocol/anthropic.rs:295-346` | 上游返回思考块时走“原始响应透传”兜底；跨协议时会把 Anthropic JSON 发给 OpenAI 客户端 |
| 5 | 跨协议流式只处理了思考文本，没有处理 `signature_delta`，也没有为工具调用保存原始 thinking 块 | `src/proxy/stream_convert.rs:225-240`、`src/proxy/stream_convert.rs:450-465` | 流式下思考内容能显示，但下一轮工具调用缺少可回传的签名块 |
| 6 | 同协议 OpenAI 流式是原样透传，`reasoning` / `reasoning_details` 不会归一化成 `reasoning_content` | `src/proxy/handler.rs:183-191` | 部分上游（OpenRouter 等）的思考字段客户端识别不了 |
| 7 | 非流式 OpenAI 响应解析只认 `reasoning_content`，不认 `reasoning` | `src/protocol/openai.rs:288-322` | OpenRouter 风格的思考内容在非流式下丢失 |
| 8 | `openai::to_request` 对所有 assistant 消息固定写 `reasoning_content: None` | `src/protocol/openai.rs:229` | 需要回传 `reasoning_content` 的上游（部分 Kimi/GLM 场景）多轮思考会断；不需要回传的上游（DeepSeek）则应保持丢弃 |
| 9 | `/v1/models` 只返回 `id/object/created/owned_by`，且只返回代理别名 | `src/proxy/handler.rs:55-63`、`src/protocol/openai.rs:381-388` | 客户端无法从接口获知思考能力；能读 `/v1/models` 的客户端拿不到能力信息 |
| 10 | 未知参数策略是全有全无的 `Auto/Drop/Passthrough`，没有区分“源协议”和“目标协议” | `src/models/route.rs:46-60` | 同协议 Anthropic 的 `metadata`、`mcp_servers` 等也会被误丢；跨协议时又可能漏掉该保留的字段 |

### 2.2 已有的可用基础

- OpenAI 请求的未知字段已经通过 `serde(flatten) extra` 保留，并在非 `Drop` 策略下透传（`src/protocol/openai.rs:25-27`、`256-263`）。
- Anthropic 请求的未知字段同样保留在 `extra`（`src/protocol/anthropic.rs:27-29`）。
- 跨协议流式已经能把 Anthropic `thinking_delta` 转成 OpenAI `reasoning_content`，反向也能把 `reasoning_content` 转成 Anthropic thinking 块（`src/proxy/stream_convert.rs:236-239`、`363-386`）。
- 后端已有 `param_overrides`，在请求体最后 merge，可作为临时强制思考参数的逃生口（`src/proxy/handler.rs:326-333`）。

### 2.3 当前可用的临时绕过

在代码改造完成前，可以在后端 `param_overrides` 中强制思考参数（注意：这是“固定档位”，不是客户端控制，且每轮都会思考，成本会上升）：

- Anthropic 后端：
  ```json
  {
    "thinking": {"type": "enabled", "budget_tokens": 16000},
    "max_tokens": 32000,
    "temperature": 1
  }
  ```
- OpenAI 推理模型后端：
  ```json
  {"reasoning_effort": "high"}
  ```
- OpenRouter 后端：
  ```json
  {"reasoning": {"effort": "high"}}
  ```

`param_overrides` 在 `to_request` 之后 merge，因此不会被 Anthropic 的未知参数 `Drop` 策略丢弃。

---

## 3. 目标与非目标

### 3.1 目标

- **G1 同协议保真**：OpenAI → OpenAI、Anthropic → Anthropic 时，思考参数与思考内容不丢失。
- **G2 跨协议映射**：`reasoning_effort` / `reasoning` / `thinking` / `enable_thinking` 在 OpenAI 与 Anthropic 之间可双向映射。
- **G3 多轮工具调用**：开启思考并使用 tools 时，后续轮次能带回 Anthropic `thinking` 块（含 `signature`），不触发 400。
- **G4 流式与非流式一致**：流式和非流式都能正确输出/接收思考内容。
- **G5 可配置的厂商风格**：不同 OpenAI 兼容上游使用不同思考参数（OpenRouter / Qwen / GLM / vLLM 等）时，可按后端选择适配方式。
- **G6 能力暴露**：`/v1/models` 提供能力元数据；前端可配置每个后端的思考风格。

### 3.2 非目标

- **不能**让 Qoder/Trae 等客户端为自定义模型显示思考开关；这是客户端模型注册表决定的。
- **不实现** OpenAI Responses API（`/v1/responses`）；如后续需要单独设计。
- **不伪造可被 Anthropic 官方验证的签名**。跨协议场景下生成的签名仅用于客户端回传，网关会识别并特殊处理，不会当作真实 Anthropic 签名使用。
- **不记录、不落盘思考内容**；签名缓存仅在内存中短期存在。

---

## 4. 总体设计

```text
客户端请求
  │
  ├─ OpenAI 协议解析 ─┐
  └─ Anthropic 协议解析 ─┤
                         ▼
              ReasoningConfig（统一中间表示）
                         │
                         ▼
        按选中 backend 的 reasoning_style 解析目标参数
                         │
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
   OpenAI effort    Anthropic thinking   OpenRouter/Qwen/...
        │                │                │
        └────────────────┼────────────────┘
                         ▼
                   上游请求（含思考参数）
                         │
                         ▼
              上游响应 / SSE 事件流
                         │
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
   OpenAI 客户端     Anthropic 客户端   原始思考块缓存
   reasoning_content   thinking blocks   （工具调用多轮用）
```

核心原则：

1. **思考参数先归一化再出站**，不再依赖 `extra` 的偶然透传。
2. **同协议优先原样保留**；跨协议才做语义映射。
3. **客户端显式请求才开启思考**；客户端没传时网关不主动注入（`param_overrides` 除外）。
4. **客户端传的 `param_overrides` 仍然最后覆盖**，保留现有手工逃生口。

---

## 5. 统一数据模型

### 5.1 新增 ReasoningConfig

在 `src/protocol/types.rs` 中新增：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningConfig {
    /// 显式开关；None 表示客户端未表态
    pub enabled: Option<bool>,
    /// 归一化档位（OpenAI 风格）
    pub effort: Option<ReasoningEffort>,
    /// token 预算（Anthropic / OpenRouter 风格）
    pub budget_tokens: Option<u32>,
    /// 是否要求返回思考摘要（OpenAI summary 等）
    pub include_thoughts: Option<bool>,
    /// 来源，用于同协议时优先原样回放
    pub source: ReasoningSource,
    /// 客户端原始字段，避免同协议下语义漂移
    pub raw: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum ReasoningSource {
    #[default]
    None,
    OpenAiEffort,
    OpenAiReasoning,
    AnthropicThinking,
    Vendor,
}
```

### 5.2 UnifiedRequest / UnifiedMessage 扩展

```rust
pub struct UnifiedRequest {
    // ...现有字段
    /// 客户端入口协议，用于决定未知参数策略和同协议回放
    pub source_protocol: Protocol,
    /// 归一化后的思考配置
    pub reasoning: Option<ReasoningConfig>,
    /// 原先 max_tokens 的字段名（max_tokens / max_completion_tokens）
    pub max_tokens_field: MaxTokensField,
}

pub struct UnifiedMessage {
    // ...现有字段
    /// Anthropic 原始 thinking / redacted_thinking 块（含 signature）
    /// 从客户端历史解析，或在跨协议响应转换时由网关生成
    pub reasoning_blocks: Vec<serde_json::Value>,
    /// Anthropic 入口的完整 assistant content 原始块序列
    /// 目标为 Anthropic 时优先原样回放，保留 thinking 与 tool_use 的交错顺序
    pub raw_blocks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum MaxTokensField {
    #[default]
    MaxTokens,
    MaxCompletionTokens,
}
```

> `Protocol` 目前定义在 `src/models/proxy.rs:26-30`，可以直接复用；但它当前没有实现 `Serialize/Deserialize`，加入 `UnifiedRequest` 时需要补 derive 或使用 `#[serde(skip)]`。`MaxTokensField` 用于 OpenAI 目标时原样输出 `max_completion_tokens`。

### 5.3 解析优先级

当客户端同时发送多个思考字段时，按以下优先级取值：

1. 显式关闭：`thinking.type = "disabled"`、`reasoning_effort = "none"`、`enable_thinking = false`
2. Anthropic 入口的 `thinking` 对象
3. OpenAI `reasoning_effort`
4. OpenAI `reasoning` 对象
5. 厂商字段：`enable_thinking` / `thinking_budget` / `chat_template_kwargs.enable_thinking`
6. 未指定

---

## 6. 入站解析

### 6.1 OpenAI 入口

新增识别并从 `extra` 中移除以下字段：

| 字段 | 说明 |
|---|---|
| `reasoning_effort` | `minimal/low/medium/high/xhigh/none`，标准 OpenAI 风格 |
| `reasoning` | OpenRouter 风格对象：`{effort, max_tokens, exclude}` |
| `thinking` | 部分 OpenAI 兼容上游使用：`{type: "enabled"/"disabled", budget_tokens}` |
| `enable_thinking` | Qwen / SiliconFlow 等布尔开关 |
| `thinking_budget` | Qwen / SiliconFlow 等的预算 |
| `chat_template_kwargs.enable_thinking` | vLLM + Qwen 模板开关 |
| `extra_body.google.thinking_config` | Gemini OpenAI 兼容层（如客户端直接放在顶层） |

同时处理：

- `max_completion_tokens` 作为 `max_tokens` 的别名，并记录 `max_tokens_field`，转发 OpenAI 时保持原字段名。
- `reasoning_content` 在 assistant 消息中保留到 `UnifiedMessage.reasoning_content`；是否回传给上游由后端策略决定（见 9.4）。

### 6.2 Anthropic 入口

新增识别：

- `thinking: {type: "enabled" | "disabled", budget_tokens: N}`。
- `output_config`（如客户端使用新版 effort 参数）暂时保留在 `extra`，后续按需适配。
- assistant 消息中的 `thinking` / `redacted_thinking` 块：
  - 完整保留原始 JSON 到 `reasoning_blocks`（包含 `signature` / `data`）。
  - 同时把整个 assistant `content` 数组原样保存到 `raw_blocks`，供同协议回放使用。
  - 继续把思考文本聚合到 `reasoning_content`，供 OpenAI 目标或非思考客户端使用。
  - 不要把 `thinking` / `redacted_thinking` 当成普通 text 块拼进 `content`。

---

## 7. 出站构造与映射

### 7.1 ReasoningStyle

在后端增加 `reasoning_style` 配置：

```rust
pub enum ReasoningStyle {
    Auto,
    OpenAiEffort,
    AnthropicThinking,
    OpenRouter,
    Qwen,
    Glm,
    VllmQwen,
    Disabled,
}
```

解析规则：

- `Auto`：
  - 平台类型为 `Anthropic` → `AnthropicThinking`
  - 平台类型为 `OpenAI/Ollama/Azure/Custom` → `OpenAiEffort`
  - 后续可根据平台预设名（OpenRouter/Qwen/GLM…）自动细化，但首版建议显式配置更稳。
- `Disabled`：忽略客户端思考参数，不向上游发送任何思考字段。
- 其他值：按对应厂商参数发出。

### 7.2 映射矩阵

| 客户端输入 | 目标风格 | 出站参数 |
|---|---|---|
| `reasoning_effort: high` | `OpenAiEffort` | `reasoning_effort: high` |
| `reasoning_effort: high` | `AnthropicThinking` | `thinking: {type: "enabled", budget_tokens: 16384}` + 约束修正 |
| `reasoning_effort: high` | `OpenRouter` | `reasoning: {effort: "high", max_tokens: 16384}` |
| `thinking: {budget_tokens: 10000}` | `OpenAiEffort` | `reasoning_effort: "medium"` |
| `thinking: {budget_tokens: 10000}` | `AnthropicThinking` | 原样保留（必要时修正 max_tokens） |
| `thinking: {budget_tokens: 10000}` | `OpenRouter` | `reasoning: {max_tokens: 10000}` |
| `thinking: {budget_tokens: 10000}` | `Qwen` | `enable_thinking: true`, `thinking_budget: 10000` |
| `thinking: {budget_tokens: 10000}` | `Glm` | `thinking: {type: "enabled"}` |
| `thinking: {budget_tokens: 10000}` | `VllmQwen` | `chat_template_kwargs: {enable_thinking: true}` |
| `reasoning_effort: none` | 任意 | 不发送开启参数；Anthropic 目标可发 `thinking: {type: "disabled"}` |
| 未传 | 任意 | 不主动发送思考参数 |

### 7.3 effort ↔ budget 映射

默认常量（后续可做成可配置）：

```rust
// effort -> budget_tokens
minimal => 1024
low     => 2048
medium  => 8192
high    => 16384
xhigh   => 32768

// budget_tokens -> effort（反向）
>= 24576 -> high（如模型支持 xhigh 可映射 xhigh）
>= 12288 -> high
>= 6144  -> medium
>= 2048  -> low
< 2048   -> minimal
```

说明：

- 这是近似映射；不同模型的最优 budget 不同，允许后续通过 `param_overrides` 覆盖。
- Anthropic 要求 `budget_tokens >= 1024`，且 `max_tokens > budget_tokens`。
- 如果客户端传的 `max_tokens` 太小，默认策略是**压缩 budget**，不静默抬高 `max_tokens`：
  ```rust
  let max = unified.max_tokens.or(default_max_tokens).unwrap_or(4096);
  let budget = mapped_budget.min(max.saturating_sub(1024));
  if budget < 1024 {
      // 关闭思考，记录 warning
  }
  ```
  > 若后续希望“高思考档位自动扩大 max_tokens”，可增加后端开关 `reasoning_expand_max_tokens`，默认关闭。

### 7.4 Anthropic 约束修正

启用 `thinking` 时，出站前必须：

- 删除 `temperature`、`top_p`、`top_k`（或至少把 `temperature` 固定为 1；建议直接删除，避免上游因采样参数报错）。
- 确保 `max_tokens > budget_tokens`。
- 检查 `tool_choice`：部分模型/版本不支持 `tool_choice: any/tool` 与 thinking 同时使用；如上游报错，直接返回明确错误，不做静默降级（或按配置降级）。
- 保留客户端原有的 `thinking` 对象（同协议时），只在字段缺失时补齐。

### 7.5 `param_overrides` 优先级

`param_overrides` 仍在最后 merge，并允许覆盖思考字段（现有行为，见 `src/proxy/handler.rs:326-333`）。这意味着：

- 路由级想强制某个思考等级时，可以用 `param_overrides`。
- 客户端控制优先于默认策略，但低于 `param_overrides`。
- 文档和前端提示中要写清楚这个优先级。

### 7.6 示例

**OpenAI 客户端 → Claude 后端**

客户端：

```json
{
  "model": "claude-sonnet-4-5",
  "reasoning_effort": "high",
  "max_tokens": 8192,
  "messages": [...]
}
```

转发：

```json
{
  "model": "claude-sonnet-4-5-20250929",
  "max_tokens": 8192,
  "thinking": { "type": "enabled", "budget_tokens": 7168 },
  "messages": [...]
}
```

（8192 - 1024 = 7168；`temperature` / `top_p` 被移除。）

**Anthropic 客户端 → OpenAI 推理模型**

客户端：

```json
{
  "model": "gpt-5",
  "thinking": { "type": "enabled", "budget_tokens": 10000 },
  "max_tokens": 16000,
  "messages": [...]
}
```

转发：

```json
{
  "model": "gpt-5",
  "max_tokens": 16000,
  "reasoning_effort": "medium",
  "messages": [...]
}
```

---

## 8. 响应与流式处理

### 8.1 Anthropic 响应解析

建议把 `AnthropicResponse.content` 改为 `Vec<serde_json::Value>`，按 `type` 手动分发，避免新增块类型导致反序列化失败。需要支持：

- `text` → `content`
- `tool_use` → `tool_calls`
- `thinking` → `reasoning_content` + `reasoning_blocks`
- `redacted_thinking` → `reasoning_blocks`（`data` 原样保留）
- 其他未知块 → 忽略或保留到 `reasoning_blocks`（视类型）

非流式 `to_response`：

- 目标是 OpenAI：输出 `reasoning_content`（可选同时输出 `reasoning`，见配置）。
- 目标是 Anthropic：优先输出原始 `reasoning_blocks`；如果没有原始块但有 `reasoning_content`，生成一个 thinking 块，并附带网关合成签名。

### 8.2 OpenAI 响应解析

`OpenAIChatResponse` 的 assistant message 需要识别：

- `reasoning_content`（DeepSeek / 多数兼容上游）
- `reasoning`（OpenRouter 字符串）
- `reasoning_details`（OpenRouter 结构化明细，可选）

前两者统一落到 `UnifiedMessage.reasoning_content`。`reasoning_details` 如需原样输出到支持它的目标，可另加独立字段保存；首版不要求。

### 8.3 同协议非流式优先透传

当前非流式路径无论客户端与上游是否同协议，都会 `parse_response` 后再 `to_response` 重建响应（`src/proxy/handler.rs:193-240`）。对于 Anthropic 同协议，这会丢失 `thinking` / `redacted_thinking` 等未建模块。

建议：

- 同协议非流式：旁路解析 usage 后，直接返回上游原始响应体（或只做最小字段修正），不重建。
- usage 旁路解析使用宽松的 JSON 取值（`raw.pointer("/usage")`），不依赖完整响应结构反序列化，避免 thinking 块导致解析失败。
- 跨协议非流式：仍走统一解析 + 转换。
- 这样 Phase 1 就能在不改响应枚举的前提下，保证 Anthropic 同协议思考块完整；响应枚举改造主要用于跨协议和统计。

### 8.4 流式转换

**Anthropic 上游 → OpenAI 客户端**

- `content_block_start`（thinking）：记录块索引，初始化文本和签名缓冲。
- `thinking_delta`：输出 `delta.reasoning_content`。
- `signature_delta`：记录签名，不向 OpenAI 客户端输出。
- `content_block_stop`：完成一个 thinking 块，保存 `{type, thinking, signature}`。
- `message_stop`：如果本次响应包含 `tool_use`，把所有 thinking 块按 `tool_use.id` 写入签名缓存。

**OpenAI 上游 → Anthropic 客户端**

- 识别 `reasoning_content`、`reasoning`，统一开始一个 Anthropic thinking 块。
- thinking 块结束时，输出一个合成的 `signature_delta`（见 9.3），再 `content_block_stop`。
- 保持 thinking 块在 text/tool_use 之前，符合 Anthropic 的常见顺序要求。

**同协议 OpenAI**

- 默认保持原样透传。
- 如果后端 `reasoning_style` 需要字段归一化（例如 OpenRouter 的 `reasoning` → `reasoning_content`），增加一个可选的重写路径，只改写思考相关字段，其余帧保持原样。

---

## 9. 多轮工具调用与签名

### 9.1 同协议 Anthropic

最简单也最可靠的路径：

- 解析 assistant 历史时，把整个 `content` 数组保存到 `raw_blocks`。
- 目标为 Anthropic 且 `raw_blocks` 存在时，**原样回放**，保留 thinking 与 tool_use 的交错顺序、`signature` 和未来新增块类型。
- 只有在目标为 OpenAI 或 `raw_blocks` 不可用时，才用 `content + reasoning_blocks + tool_calls` 重建。
- 重建时按 thinking 块 → text 块 → tool_use 块的顺序输出；`signature` 不做任何修改。

这条路径可以覆盖 Claude Code 等 Anthropic 原生客户端，应优先完成。

### 9.2 跨协议：Anthropic 上游 → OpenAI 客户端

OpenAI 客户端不会回传 `signature`，因此需要网关短期缓存：

```text
Anthropic 响应（thinking + tool_use）
        │
        ├─ 转成 OpenAI 的 reasoning_content / tool_calls
        └─ 缓存 tool_use.id -> [原始 thinking blocks]
                    │
客户端下一轮发回 tool_calls + tool result
                    │
        └─ 网关按 tool_call.id 查缓存
                    │
        └─ 在发给 Anthropic 的 assistant 消息中注入 thinking blocks
```

缓存设计：

- 存储：`DashMap<String, CacheEntry>`，`CacheEntry { blocks: Vec<Value>, inserted_at: Instant }`
- Key：`tool_use.id`（Anthropic 的 tool id 全局唯一，足够区分）
- TTL：30 分钟（可配置）
- 容量：最多 4096 条，超出时清理过期项和最旧项
- 仅内存，不持久化
- 不记录思考内容到日志

请求侧注入规则：

- 对每个带 `tool_calls` 的 assistant 消息，取第一个 `tool_call.id` 查缓存。
- 如果客户端已经带了 `reasoning_blocks`（Anthropic 原生客户端），优先使用客户端块，不查缓存。
- 如果查不到缓存：
  - 有工具调用且目标 Anthropic：**关闭本次请求的 thinking**（fail-open），记录 warning，避免 Anthropic 因缺少 thinking 块返回 400。
  - 没有工具调用：可以正常开启 thinking，不需要历史块。

### 9.3 跨协议：OpenAI 上游 → Anthropic 客户端

- OpenAI 上游没有真实 Anthropic 签名。
- 网关在输出 thinking 块时生成一个内部合成签名，例如 `gw_sig_<random>`。
- 客户端后续把该块回传时：
  - 目标是 OpenAI：忽略思考块和签名，只把需要的内容转成 OpenAI 历史（默认不带 `reasoning_content`）。
  - 目标是 Anthropic（混合后端路由）：识别到 `gw_sig_` 前缀，丢弃该合成块；若因此导致带工具的 assistant 消息缺少 thinking 块，则关闭本次 thinking（fail-open）。
- 合成签名**绝不能**直接发给真实 Anthropic 上游。

### 9.4 OpenAI assistant 历史中的 reasoning_content

不同上游策略不同：

- DeepSeek Reasoner：明确不应把 `reasoning_content` 回传到输入。
- 部分 Kimi / GLM / 其他兼容上游：多轮工具调用时可能要求回传。

首版默认保持现状（不回传），并增加一个后端配置项：

```rust
pub enum ReasoningEcho {
    Drop,   // 默认：不把 assistant reasoning_content 发给上游
    Keep,   // 原样回传
    Auto,   // 预留：按模型/平台判断
}
```

该配置只影响 OpenAI 目标；Anthropic 目标使用 `reasoning_blocks` 的规则。

### 9.5 路由切换与签名绑定

Anthropic 的 thinking 签名由具体模型/上游生成，跨 backend 复用存在风险。若一个 route 配置了多个 Anthropic 模型或多个账号：

- 工具轮次之间可能被负载均衡切到另一个 backend。
- 把上一个模型生成的 thinking block 发给另一个模型，可能被 Anthropic 拒绝。

建议：

- 缓存条目同时记录 `backend_id` / `model_id`。
- 注入前校验当前选中的 backend 与缓存来源是否一致；不一致时：
  - 关闭本次 thinking（fail-open），或
  - 按配置直接返回明确错误。
- 后续可考虑给“带 thinking 的工具对话”增加会话粘性（sticky backend），但会增加调度复杂度，暂不列入首版。

---

## 10. 未知参数策略调整

现有 `UnknownParams`（`src/models/route.rs:46-60`）只按目标平台判断，建议改为“源协议 + 目标协议”二维策略：

| 源 → 目标 | Auto 行为 |
|---|---|
| OpenAI → OpenAI | 透传未知字段（保持现状） |
| OpenAI → Anthropic | 仅保留 Anthropic 允许的字段；丢弃 OpenAI 特有字段（`response_format`、`seed`、`logprobs` 等） |
| Anthropic → Anthropic | 透传未知字段（同协议，保留 `metadata`、`mcp_servers`、`context_management` 等） |
| Anthropic → OpenAI | 透传未知字段，但剔除 Anthropic 特有字段（`top_k`、`stop_sequences`、`metadata` 等） |
| `Drop` | 强制白名单模式（调试/严格场景） |
| `Passthrough` | 强制原样透传（最高风险，仅调试） |

实现要点：

- `UnifiedRequest` 增加 `source_protocol`。
- `UnknownParams::Auto` 在 `to_request` 内按上表处理。
- `thinking`、`reasoning_effort` 等已建模字段不再进入 `extra`，避免被二次透传或错误丢弃。
- 同协议 Anthropic 默认不再执行“全量 Drop”，避免误伤原生 Anthropic 功能。

---

## 11. 能力暴露与前端

### 11.1 `/v1/models`

在现有字段基础上增加（非标准扩展，能读的客户端会受益）：

```json
{
  "id": "claude-sonnet-4-5",
  "object": "model",
  "created": 0,
  "owned_by": "ai-gateway",
  "capabilities": ["Reasoning", "Vision", "Tool"],
  "supported_parameters": ["tools", "tool_choice", "reasoning_effort", "thinking"],
  "reasoning": {
    "efforts": ["low", "medium", "high"],
    "budget_tokens": true
  }
}
```

实现：

- 当前 `openai_list_models` 只查 `db::proxy::list`（`src/proxy/handler.rs:55-63`）。
- 改为使用 `RouteCache` 中已解析的路由/后端信息，或在 DB 查询时联表。
- 能力取各后端 `capabilities` 的交集；如果为空，不输出该字段。
- 注意：Qoder/Trae 等客户端大概率不读取这些字段，不能指望它解锁 UI。

### 11.2 前端后端配置

在 `frontend/src/pages/Proxies.tsx` 的后端行中增加“思考适配”：

- 字段：`reasoning_style`
- 选项：
  - `Auto`（按平台自动）
  - `OpenAI effort`（标准 `reasoning_effort`）
  - `Anthropic thinking`（`thinking.budget_tokens`）
  - `OpenRouter`（`reasoning`）
  - `Qwen`（`enable_thinking` + `thinking_budget`）
  - `GLM`（`thinking.type`）
  - `vLLM Qwen`（`chat_template_kwargs.enable_thinking`）
  - `Disabled`（不转发思考参数）
- 提示文案：客户端传什么就映射什么，未传不主动开启；`param_overrides` 仍然最高优先级。

`BackendRow`、表单重置、编辑加载、提交（`frontend/src/pages/Proxies.tsx:24-34`、`281-289`、`316-326`、`433-443`、`853-875`）都要同步。

### 11.3 能力标签

`Reasoning` 能力已存在（`frontend/src/presets.ts:166-172`），无需新增枚举；可在 UI 上显示为“推理/思考”。

---

## 12. 数据库变更

新增迁移 V9（`src/db/schema.rs`）：

```sql
-- 思考参数适配风格：Auto / OpenAiEffort / AnthropicThinking / OpenRouter / Qwen / Glm / VllmQwen / Disabled
ALTER TABLE backends ADD COLUMN reasoning_style TEXT NOT NULL DEFAULT 'Auto';

-- 可选：assistant reasoning_content 是否回传（Drop / Keep / Auto）
ALTER TABLE backends ADD COLUMN reasoning_echo TEXT NOT NULL DEFAULT 'Drop';
```

同步修改：

- `src/models/route.rs` 的 `Backend`、`CreateBackendRequest`、`UpdateBackendRequest`。
- `src/db/route.rs` 的所有 SELECT / INSERT / UPDATE 语句。
- `src/db/schema.rs` 的 `run_migrations()` 增加 `current_version < 9` 分支。
- 旧数据库升级后默认 `Auto` / `Drop`，行为与当前一致。

---

## 13. 分阶段实施

### Phase 1：同协议保真（优先，风险最低）

目标：Anthropic 原生客户端（如 Claude Code）通过网关能正常使用 extended thinking + tools。

改动：

- `src/protocol/types.rs`：新增 `ReasoningConfig`、`reasoning_blocks`、`source_protocol`。
- `src/protocol/anthropic.rs`：
  - 解析请求 `thinking`。
  - 保留 assistant 历史中的 thinking / redacted_thinking 原始块和 `signature`。
  - 保存完整 `raw_blocks`；同协议重建时优先原样回放。
  - 响应解析支持 thinking / redacted_thinking。
- `src/proxy/handler.rs`：
  - 把 `source_protocol` 传入统一请求。
  - 同协议非流式改为优先透传原始响应体。
- `src/proxy/stream_convert.rs`：同协议流式本来透传，补充测试即可。

验收：

- `/v1/messages` 请求带 `thinking`，上游收到。
- 流式/非流式响应中的 thinking 块完整。
- 连续工具调用第二轮不返回 400。

### Phase 2：跨协议映射

目标：OpenAI 客户端可以用 `reasoning_effort` 控制 Claude 后端；Anthropic 客户端可以用 `thinking.budget_tokens` 控制 OpenAI 推理模型。

改动：

- `src/protocol/openai.rs`：解析 `reasoning_effort` / `reasoning` / 厂商字段；输出目标风格参数。
- `src/protocol/anthropic.rs`：`thinking` ↔ `reasoning_effort` / `reasoning` 映射；Anthropic 约束修正。
- 新增 `src/proxy/reasoning.rs`（或 `src/protocol/reasoning.rs`）：
  - `ReasoningStyle` 解析
  - effort ↔ budget 映射
  - max_tokens / temperature / top_p / top_k 修正
  - 合成签名生成与识别
- `src/proxy/handler.rs`：`build_forward_request` 使用 `reasoning_style`。
- `src/models/route.rs` + DB V9 + 前端：新增 `reasoning_style`。

验收：

- OpenAI → Claude：`reasoning_effort: high` 转成合理的 `thinking.budget_tokens`，响应回到 `reasoning_content`。
- Claude → OpenAI：`thinking.budget_tokens` 转成合理 `reasoning_effort`，响应回到 thinking 块。
- 客户端显式 `none/disabled` 时不开启思考。
- `param_overrides` 仍可覆盖。

### Phase 3：多轮工具调用签名

目标：跨协议 + tools + thinking 的连续对话可用。

改动：

- 新增 `src/proxy/thinking_cache.rs`：
  - `tool_use.id -> Vec<Value>` 缓存
  - TTL / 容量 / 清理
- `src/proxy/handler.rs`：
  - 非流式响应转换时写入缓存。
  - 请求转换前按 tool_call.id 注入 `reasoning_blocks`。
  - 缓存缺失时 fail-open（关闭 thinking）。
- `src/proxy/stream_convert.rs`：
  - 收集 `thinking_delta` / `signature_delta`。
  - 在 `message_stop` 时写入缓存。
  - 输出侧生成/识别合成签名。
- `src/proxy/handler.rs` 的 `StreamConvertBody` 持有 `Arc<ThinkingCache>`；`ProxyState` 增加该字段（`src/main.rs:48-55` 初始化）。

验收：

- OpenAI 客户端 → Claude 后端，带 tools 的多轮思考对话不 400。
- 网关重启/缓存过期后，带工具的请求自动关闭 thinking 并正常返回，不阻塞对话。
- 混合后端路由不会把合成签名发给真实 Anthropic。

### Phase 4：厂商风格与能力暴露

目标：覆盖 OpenRouter / Qwen / GLM / vLLM 等常见 OpenAI 兼容上游；`/v1/models` 提供能力信息。

改动：

- `src/protocol/openai.rs`：按 `reasoning_style` 输出不同字段。
- `src/proxy/handler.rs`：`openai_list_models` 返回 capabilities / supported_parameters / reasoning。
- 前端：思考风格下拉、提示文案、i18n。
- 可选：`reasoning_echo` 配置（默认 Drop）。

验收：

- 同一代理切换不同 reasoning_style 后，抓包确认请求体字段符合目标厂商。
- `/v1/models` 返回能力元数据，标准客户端不受影响。
- Qoder/Trae 回归测试无异常；UI 是否显示思考开关仍由客户端决定。

### Phase 5：观测与文档

- debug 日志：记录映射前后字段、budget、约束修正、缓存命中/未命中（不记录思考内容）。
- 可选：响应头 `x-ai-gateway-reasoning`，便于排查客户端到底发了什么。
- 更新 README / AGENTS：支持的思考参数、风格、优先级、已知限制。
- 可选：请求统计增加 reasoning tokens（如果上游 usage 提供）。

---

## 14. 测试计划

### 14.1 单元测试（`cargo test`）

建议新增用例：

- OpenAI 请求：
  - `reasoning_effort` 解析为 `ReasoningConfig`。
  - `reasoning` 对象解析。
  - `thinking` / `enable_thinking` / `thinking_budget` 解析。
  - `max_completion_tokens` 别名。
- OpenAI → Anthropic：
  - effort → budget 映射正确。
  - max_tokens 约束与 budget 压缩。
  - `temperature` / `top_p` / `top_k` 被移除。
  - `none` / `disabled` 不产生 thinking。
- Anthropic → OpenAI：
  - budget → effort 映射。
  - `reasoning_content` 输出。
  - thinking 块转 OpenAI 非流式响应。
- Anthropic 同协议：
  - `thinking` 请求参数原样保留。
  - assistant 历史 thinking 块 + signature 顺序保持。
  - `redacted_thinking` 保留。
  - 非流式响应解析不再失败。
- 流式转换：
  - `thinking_delta` / `signature_delta`。
  - OpenAI `reasoning` / `reasoning_content` 两种字段。
  - thinking 块在 text/tool_use 之前的顺序。
- 未知参数策略：
  - Anthropic → Anthropic 保留 `metadata`。
  - OpenAI → Anthropic 丢弃 `response_format`。
  - Anthropic → OpenAI 丢弃 `top_k`。
- Thinking 缓存：
  - 插入 / 查找 / 过期 / 容量淘汰。
  - tool_call.id 注入。
  - 缓存缺失时关闭 thinking。

### 14.2 手工联调

| 场景 | 入口 | 上游 | 验证点 |
|---|---|---|---|
| Claude Code 思考 + 工具 | `/v1/messages` | Anthropic | thinking 块、signature、多轮不 400 |
| OpenAI SDK 思考 | `/v1/chat/completions` | OpenAI o 系 / GPT-5 | `reasoning_effort` 到达上游，响应正常 |
| OpenAI 客户端控制 Claude | `/v1/chat/completions` | Anthropic | effort → thinking，响应 `reasoning_content` |
| Anthropic 客户端控制 OpenAI | `/v1/messages` | OpenAI 推理模型 | budget → effort，响应 thinking 块 |
| OpenRouter | `/v1/chat/completions` | OpenRouter | `reasoning` 对象 |
| Qwen / vLLM | `/v1/chat/completions` | DashScope / vLLM | `enable_thinking` / `chat_template_kwargs` |
| Qoder / Trae | 自定义模型 | 任意 | 无回归；抓包确认是否发送思考参数 |

### 14.3 回归

- 普通文本对话（流式/非流式）。
- 工具调用（不开启思考）。
- 图片输入（`normalize_images_in_body`）。
- usage 统计（输入/输出/缓存命中）。
- `/v1/models` 标准字段兼容性。
- 前端代理创建/编辑/保存。
- `cargo test`、`cargo build`、`cd frontend && npm run build`。

---

## 15. 风险与回滚

| 风险 | 说明 | 缓解 |
|---|---|---|
| Qoder/Trae 仍不显示开关 | 客户端模型能力表写死，网关无法解锁 | 明确为非目标；通过抓包确认客户端是否发送参数 |
| Anthropic 版本差异 | budget 下限、max_tokens 约束、工具调用与 thinking 的兼容性可能随模型版本变化 | 真实账号联调；约束修正逻辑集中在一个模块，便于调整 |
| 错误映射导致 400 | 某上游不接受 `reasoning_effort` 或字段形状不同 | `reasoning_style` 按后端配置；默认只发客户端显式请求的字段；`Disabled` 可关闭 |
| 合成签名被误用 | 混合后端路由下，把 `gw_sig_` 发给真实 Anthropic | 识别前缀并丢弃；目标为 Anthropic 且缺少真实块时 fail-open 关闭 thinking |
| max_tokens 被压缩 | 高 effort 在低 max_tokens 下被降级 | 记录 warning；提供 `param_overrides` 手动调高；后续可加 `reasoning_expand_max_tokens` |
| 签名缓存内存增长 | 长会话/大量工具调用 | TTL + 容量上限 + 最旧淘汰；仅内存 |
| 网关重启导致缓存丢失 | 跨协议工具轮次可能缺 thinking 块 | 缓存未命中时关闭 thinking，保证对话继续 |
| 负载均衡切换 backend | thinking 签名可能只对原模型有效 | 缓存记录来源 backend/model，不一致时关闭 thinking 或报错；后续考虑会话粘性 |
| 成本/延迟上升 | 客户端开启思考后 token 增加 | 客户端自主控制；网关不主动开启 |

回滚策略：

- 新增的 `reasoning_style` 默认 `Auto`，旧数据升级后行为可控。
- 如出现问题，可将相关后端设为 `Disabled`，立即恢复“只转发不映射”的旧行为。
- 新字段和逻辑独立于现有转发主流程，可按 Phase 回退。

---

## 16. 待确认问题

1. 首批要支持哪些厂商的 `reasoning_style`？建议优先级：OpenAI / Anthropic → OpenRouter → Qwen → GLM → vLLM。
2. `max_tokens` 太小时，是压缩 budget（默认）还是自动扩大 max_tokens？
3. 跨协议签名缓存未命中时，fail-open 关闭 thinking 是否可接受，还是希望直接报错让用户感知？
4. `/v1/models` 的能力字段是否按交集计算？部分后端未填 capabilities 时是否退回并集？
5. 是否需要支持 OpenAI Responses API（`/v1/responses`）？当前大量新 Agent 使用该协议，但不在本方案范围。
6. Qoder/Trae 实际发送的思考字段需要抓包确认；如果它们根本不发送，网关侧改造无法让 UI 出现开关。

---

## 17. 结论

优先级最高的不是“让 Qoder/Trae 显示开关”，而是先把网关的思考链路修通：

1. **Phase 1 修同协议 Anthropic**：解决 `thinking` 被 Drop、签名丢失、工具轮次断裂。
2. **Phase 2 修跨协议映射**：让 `reasoning_effort` / `thinking` 双向可用。
3. **Phase 3 修跨协议工具轮次**：用短期缓存补回 Anthropic thinking 块。
4. **Phase 4 补厂商适配和能力暴露**：覆盖 OpenRouter/Qwen/GLM 等，方便更多客户端识别。

完成 Phase 1-3 后，只要客户端愿意发送思考参数，网关就能端到端支持；Qoder/Trae 是否展示开关仍取决于它们自己的模型注册表。

---

## 18. 落地状态（2026-10-20）

Phase 1-5 已全部落地，`cargo test` 35 项通过，`cd frontend && npm run build` 通过。

| 阶段 | 状态 | 主要落点 |
|---|---|---|
| Phase 1 同协议保真 | ✅ | `src/protocol/anthropic.rs` 解析/回放 thinking 与 redacted_thinking，同协议非流式由 `handle_request` 原样透传并改写 `model` |
| Phase 2 跨协议映射 | ✅ | 新增 `src/protocol/reasoning.rs`；OpenAI `reasoning_effort`/`reasoning`/厂商字段 ↔ Anthropic `thinking`；budget 压缩、采样参数剥离 |
| Phase 3 多轮工具签名 | ✅ | 新增 `src/proxy/reasoning_cache.rs`（全局 DashMap，30 分钟 TTL / 4096 条上限 / backend+model 绑定）；非流式与流式均写入；缺失时 fail-open |
| Phase 4 厂商风格与能力 | ✅ | `reasoning_style` 六种厂商风格 + Disabled；`/v1/models` 返回 capabilities / supported_parameters / reasoning；前端后端配置与中英文案 |
| Phase 5 观测与文档 | ✅ | `tracing::debug!` 记录思考映射参数与缓存命中/未命中（不记录思考内容）；README / AGENTS 更新 |

与设计文档的差异：

- 签名缓存使用全局 `static`（`reasoning_cache.rs`）而非 `ProxyState` 字段，行为一致、改动面更小。
- Phase 5 的可选项（响应头 `x-ai-gateway-reasoning`、统计 reasoning tokens）未实现，不影响思考链路。

关键文件：

- 统一模型：`src/protocol/reasoning.rs`
- 入站/出站：`src/protocol/openai.rs`、`src/protocol/anthropic.rs`
- 路由与透传：`src/proxy/handler.rs`
- 流式转换：`src/proxy/stream_convert.rs`
- 签名缓存：`src/proxy/reasoning_cache.rs`
- 配置与迁移：`src/models/route.rs`、`src/db/route.rs`、`src/db/schema.rs`（V9）
- 前端：`frontend/src/pages/Proxies.tsx`、`frontend/src/i18n.ts`
