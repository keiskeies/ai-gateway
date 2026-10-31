# 请求前置随机串（request nonce）设计

> 状态：待实现　|　日期：2026-10-31　|　关联：路由配置「请求前置随机串」开关

## 1. 目标

在虚拟模型的路由配置里加一个开关。打开后，**每次发往上游的请求**在最前面插入一串随机数字
（长度 32~128 位、长度本身也随机），使上游看到的第一批 token 每次都不相同。

用途是实验/规避性手段（例如验证上游是否按前缀做去重或缓存）；**不是**限流修复手段——
2026-10-30 的实测已经证明那批 429 是上游随机丢包，与前缀无关。

明确不做：

- 不改客户端看到的消息（只在出站请求上生效，回放/审计里能看出是网关加的）。
- 不做多种随机策略（字母/时间戳/UUID）。就是字母数字混合（大小写+数字），32~128 位。
- 不做长度可配。写死在 32~128，改范围要发版。
- 不动 `param_overrides` 的语义（两者可共存：覆盖仍然最后 merge）。

## 2. 已确认的代码事实

| 事实 | 位置 |
| --- | --- |
| 所有标准协议请求只有一个出站组装点，每次尝试都经过 | `src/proxy/handler.rs:795` → `build_forward_request` (`:1149`) |
| 私有协议（Trae / WorkBuddy）走 `upstream::build`，**不经过** `build_forward_request` | `handler.rs:788` |
| `anthropic::to_request` 把 system 写成顶层 `system` 字段，并跳过 `Role::System` 消息 | `src/protocol/anthropic.rs:315,324` |
| `openai::to_request` **只渲染 `messages`**，完全不读 `unified.system` | `src/protocol/openai.rs:281-310` |
| OpenAI 入口把 system 同时写进 `unified.system` 和 `messages[0]` | `src/protocol/openai.rs:167-176` |
| Anthropic 入口只写 `unified.system`，messages 里没有 system | `src/protocol/anthropic.rs:190` |
| `routes` 表列：id / proxy_id / lb_strategy / retry_policy / fallback | `src/db/schema.rs:614` |
| 迁移助手与版本常量 | `src/db/schema.rs:313` `add_column_if_missing`、`CURRENT_SCHEMA_VERSION` |
| 配置导出有独立的列清单 | `src/db/config_transfer.rs:19` `ROUTE_COLS`、`:491`、`:609` |

### 2.1 由此得到的核心约束

**「最前面」在两种目标协议上是两个不同的字段**：

- Anthropic 目标 → 渲染 `unified.system`（顶层字段）；
- OpenAI 目标 → 只渲染 `unified.messages`。

所以注入必须**按目标协议的渲染面**选择位置，否则会出现「注入了但没在出站正文里」的情况。

### 2.2 顺带确认的既有缺陷（一并修）

实测（临时单测，已回滚）：

```
anthropic::parse_request({"system":"你是严谨的助手", ...}) → openai::to_request
出站 messages 的 role 序列 = ["user"]      ← system 整段丢失
```

即 **Anthropic 客户端（Claude Code 等）路由到 OpenAI 兼容上游时，system prompt 不会发出去**。
这是本次要动的同一个函数里的正确性问题，按「相邻缺陷一起修」纳入本次范围（见 §6）。

## 3. 配置与存储

新增 `routes.request_nonce INTEGER NOT NULL DEFAULT 0`（0 = 关，1 = 开）。

- 迁移：V29，用 `add_column_if_missing` 加列并 `UPDATE _schema_version`。
- `Route` / `ResolvedRoute` 各加 `pub request_nonce: bool`（`#[serde(default)]`，旧行读出来是 false）。
- `CreateRouteRequest` / `UpdateRouteRequest` 加 `request_nonce: Option<bool>`。
- `db/route.rs` 的四处 SELECT / INSERT / UPDATE 补列；`config_transfer.rs` 的 `ROUTE_COLS`
  与两处 INSERT 同步补列（导出/导入不能丢开关）。
- 缓存：`RouteCache` 直接由 `db::route::get_by_proxy` 构建，字段自动跟随，无需额外代码。

## 4. 注入规则

### 4.1 随机串

```
len = 32 + (rand % 97)          // 32..=128，含两端
nonce = len 位十进制数字
```

每次**上游尝试**重新生成（换号重试时旧前缀已经"脏"了，重新生成才有意义）。

### 4.2 放在哪

按目标协议选择渲染面：

| 目标 | surface | 规则 |
| --- | --- | --- |
| Anthropic | `SystemField` | `unified.system` 有值 → 拼成 `"{nonce}\n{原文}"`；无值 → 在最前面插一条只含 nonce 的 user 消息 |
| OpenAI 兼容 | `Messages` | `messages[0].role == system` → 拼到该消息文本最前；否则 → 最前面插一条只含 nonce 的 user 消息 |
| 私有协议（Trae/WorkBuddy） | `SystemField` | 适配器读 `unified.system`；规则同上 |

文本可能是字符串或内容块数组：数组时拼到**第一个 text 块**的 `text` 上（无 text 块则插一条消息）。
`content` 为 null（assistant + tool_calls）时不改，直接走"插消息"分支。

### 4.3 实现位置

在 `handler.rs` 的尝试循环里，`upstream::build` 之前：

```rust
let mut attempt_unified;                       // 仅开关打开时使用
let unified_ref = if nonce_enabled {
    attempt_unified = unified.clone();
    nonce::apply(&mut attempt_unified, surface_for(&resolved.platform, adapter));
    &attempt_unified
} else { &unified };
```

然后 `upstream::build(...)` 与 `build_forward_request(...)` 都传 `unified_ref`。
这样私有协议与标准协议共用一套注入逻辑，且每次尝试只生成一次 nonce。

新模块 `src/protocol/nonce.rs`，只暴露：

- `pub enum Surface { SystemField, Messages }`
- `pub fn apply(unified: &mut UnifiedRequest, surface: Surface)`
- `pub(crate) fn random_digits() -> String`（供单测断言长度/字符集）

## 5. 前端

「路由配置」面板，`默认 max_tokens` 之后加：

- `Switch`「请求前置随机串」（i18n：`nonceToggle` / `nonceToggleHint`）
- 副文案写明代价：**提示缓存会全部失效，每次请求按全价输入计费、TTFT 变长**。
- 打开状态不额外加数字框（长度写死）。
- `routeDraft` 增加 `nonce: boolean`，随 `saveRoute` 一起提交；`ModelDetail` 的 `detail` 回填遵循
  已有的 `routeDraftTouched` 规则（用户改过就不覆盖）。

## 6. 相邻缺陷：Anthropic → OpenAI 丢 system

`openai::to_request` 增加一步：若 `unified.system` 有值**且** `messages` 里没有任何
`Role::System` 消息，则在 `messages` 最前面插入一条 `{"role":"system","content": system}`。

- 不重复：OpenAI 源请求已经带 system 消息 → 不插。
- 顺序：插在索引 0，早于所有用户消息。
- 这同时让 §4.2 的注入在跨协议场景下也落在"最前面"。

## 7. 测试

单元测试（`src/protocol/nonce.rs`）：

1. 长度：1000 次生成，全部落在 32..=128 且全为数字。
2. `SystemField` + 有 system：system 以 nonce 开头，原文完整保留。
3. `SystemField` + 无 system：messages 长度 +1，首条为 user 且内容就是 nonce。
4. `Messages` + 首条 system：该条文本以 nonce 开头。
5. `Messages` + 首条 system 且 content 是块数组：拼在第一个 text 块。
6. `Messages` + 首条不是 system：插入首条 user。
7. 连续调用两次结果不同（随机性）。
8. 注入不改变 tools / tool_calls / tool_call_id（回放结构不被破坏）。

跨协议回归（`src/protocol/openai.rs`）：

9. `anthropic::parse_request` → `openai::to_request`：出站 `messages[0].role == "system"` 且文本一致。
10. OpenAI 源 → OpenAI 目标：system 只出现一次（不重复）。

数据库（`src/db/route.rs`）：

11. 新建路由默认 `request_nonce=false`；`update_route` 能开能关；老库迁移后读出 false。

端到端（隔离实例）：开开关 → 连发两条相同请求 → 抓上游收到的两条正文，前缀不同且长度在 32~128。

## 8. 风险

| 风险 | 说明 | 处理 |
| --- | --- | --- |
| 提示缓存全失效 | 成本上升、TTFT 变长 | UI 文案明确写出；默认关闭 |
| 每次尝试正文不同 | 同一次用户请求在日志里有两条不同 body | 预期行为；`request_attempts` 本来就逐次记录 |
| 迁移写错列 | 老库读不到开关 | `add_column_if_missing` + 迁移测试；只加列不改列 |
| 配置导出丢开关 | 换机后开关静默回默认 | `ROUTE_COLS` 与两处 INSERT 同步补列 |
| 注入破坏工具回放 | tool_call_id 配对错乱 | 只改 system/首条消息文本，单测覆盖 tools 保留 |

回滚：开关默认关，关掉即恢复原行为；迁移只加列，回滚 exe 后新列被忽略。

## 9. 交付顺序

1. 迁移 + 模型/DB 字段 + config_transfer（可独立验证：`cargo test`）。
2. `nonce` 模块 + 注入接线 + 单测。
3. `openai::to_request` 的 system 修复 + 回归测试。
4. 前端开关 + i18n。
5. 隔离实例端到端验证；再决定是否部署。
