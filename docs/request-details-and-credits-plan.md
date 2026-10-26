# 请求详情日志与上游积分额度 · 设计与落地计划

> 目标：在不改变现有转发协议、缓存结构和统计口径的前提下，补上两类能力：
>
> 1. **请求详情**：入站路径、转发地址、转发模型、请求头、请求/响应正文、保留天数；
> 2. **上游积分额度**：平台总积分（按天/周/月重置）+ 模型单次积分单价 + 选路跳过耗尽平台 + 消耗台账与告警。
>
> **结论先行**：请求详情在现有 `request_stats` / `request_attempts` / `request_bodies` 上增量扩展，不新建第二套日志；积分必须用**独立台账**（`platform_credit_state` + `platform_credit_ledger`），不能从 `request_stats` 推导，否则清理历史会把积分“退回来”。

---

## 0. 范围与不做的部分

### 0.1 本次做

| 编号 | 能力 | 交付 |
|---|---|---|
| A1 | 入站路径落库 | `/v1/chat/completions`、`/v1/completions`、`/v1/messages` |
| A2 | 转发地址落库 | 每次尝试一行，含重试/换后端，转发前打码 |
| A3 | 请求头落库 | 白名单 + 强制排除敏感头，仅在采集开启时 |
| A4 | 响应正文落库 | 非流式与流式都支持，64KB 截断，单独开关 |
| A5 | 详情查看 | 请求头 / 请求正文 / 响应正文 / 每次尝试的转发地址 |
| A6 | 自动保留期 | 设置页配置，默认关，启动 + 每 24h 清理，最短 30 天 |
| B1 | 平台积分预算 | `platforms.credit_budget` + `credit_window`，自然日/周/月重置 |
| B2 | 模型积分单价 | `models.credit_price`，按成功请求计费 |
| B3 | 积分感知选路 | 选后端前过滤耗尽平台；全部耗尽返回 429 + 重置时间 |
| B4 | 积分台账 | `platform_credit_state` + `platform_credit_ledger`，与日志清理解耦 |
| B5 | 明细与告警 | 请求明细积分列、平台积分读数、低额度/耗尽告警、手动校准 |

### 0.2 本次不做

- 不实现 `routes.fallback`、`backend.max_concurrent`、`platform.rate_limit`、真正的延迟优先选路；这些是独立的“半成品字段”修复项，不混进本次。
- 不做多 Key 池、上游余额接口、语义缓存、团队/多租户。
- 不做响应正文的协议转换预览：存的是**上游原始响应**，不是客户端最终收到的转换结果。
- 不做自动 `VACUUM`：删除只释放页，不缩文件；先记录为已知限制。

---

## 1. 现状核对（落地前的事实）

| 事实 | 位置 | 对设计的影响 |
|---|---|---|
| `request_stats` 已有 `request_id`、`cost`、`api_key_id`、`replayed_from` | `src/db/schema.rs` V1/V12/V15/V16/V21 | 路径与积分直接加列，不新建日志表 |
| `request_attempts` 每次尝试一行，含 `model_id`、`platform_id`、错误摘要 | `src/db/schema.rs` V16 | 转发地址加在 attempt 上，而不是 stats 上 |
| `request_bodies` 以 `request_id` 为主键，只存入站正文，64KB 上限，超限不存 | `src/db/schema.rs` V21、`src/proxy/handler.rs` | 加 `headers` / `truncated`；响应正文另表，避免破坏重放 |
| 流式响应由 `StreamPassthroughBody` / `StreamConvertBody` 包装 | `src/proxy/handler.rs` | 在这两个 `poll_next` 里旁路截断采集原始 SSE |
| 统计写入走 `StatsWriter` 无界通道 + 500ms 批量 | `src/cache.rs` | 正文采集继续走同一通道，绝不在请求线程写 SQL |
| Key 配额从 `request_stats` 按滚动窗口统计 | `src/proxy/handler.rs`、`src/models/api_key.rs` | 自动清理若短于 30 天会误清月度用量 → 自动保留期下限 30 天 |
| 平台/模型配置可由配置导出/导入带走 | `src/db/config_transfer.rs`、`src/models/config_transfer.rs` | 积分预算与积分单价属于配置，必须加入导出/导入 |
| 告警是“读时扫描” | `src/db/alert.rs::list` | 积分告警加在扫描器里，不需要新定时任务 |
| 平台/模型更新后会 `cache.refresh()` | `src/api/platform.rs`、`src/api/model.rs` | 积分字段改动同样刷新缓存，避免选路读旧值 |
| Tauri 与独立服务各自启动一次 actix | `src/main.rs`、`src-tauri/src/lib.rs` | 自动清理任务要抽成共享函数，两处都调用 |

---

## 2. 数据库迁移

新增 **V23（请求详情）** 与 **V24（积分）**，`CURRENT_SCHEMA_VERSION = 24`。
两个迁移分开，便于失败时定位；`_schema_version` 的推进方式沿用现有 `run_migrations`。

### 2.1 V23：请求详情

```sql
-- 入站路径（/v1/chat/completions 等）；旧行为 NULL
ALTER TABLE request_stats ADD COLUMN path TEXT;

-- 每次尝试的上游完整地址（已打码）；旧行为 NULL
ALTER TABLE request_attempts ADD COLUMN forward_url TEXT;

-- 入站请求头（JSON 对象，已白名单过滤）；仅在采集开启时写入。
-- 独立成表而不是挂 request_bodies：正文超过 64KB 时不存正文，但请求头仍然有价值，
-- 挂在正文行上会让长请求连头都一起丢掉。
CREATE TABLE IF NOT EXISTS request_headers (
    request_id TEXT PRIMARY KEY,
    headers    TEXT NOT NULL,
    bytes      INTEGER NOT NULL,
    truncated  INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

-- 上游原始响应正文；一次请求一行
CREATE TABLE IF NOT EXISTS response_bodies (
    request_id  TEXT PRIMARY KEY,
    protocol    TEXT NOT NULL,          -- 上游协议：openai / anthropic
    status_code INTEGER NOT NULL,
    content_type TEXT,
    body        TEXT NOT NULL,
    bytes       INTEGER NOT NULL,
    truncated   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);
```

**为什么响应正文不塞进 `request_bodies`**：后者是重放入口，主键是 `request_id`，语义是“客户端发来的原始请求”；把响应塞进去会让重放查询、清空正文、保留期删除三处都要先判类型。单独一张表，删除时并列处理即可。

### 2.2 V24：积分

```sql
-- 平台预算。NULL = 不启用积分；credit_window 只允许 day/week/month
ALTER TABLE platforms ADD COLUMN credit_budget REAL;
ALTER TABLE platforms ADD COLUMN credit_window TEXT;

-- 模型单次请求积分价。NULL = 未定价，按 0 计
ALTER TABLE models ADD COLUMN credit_price REAL;

-- 当前窗口用量。独立台账，不跟 request_stats 一起删
CREATE TABLE IF NOT EXISTS platform_credit_state (
    platform_id  TEXT PRIMARY KEY,
    period_start TEXT NOT NULL,        -- 当前自然窗口起点，RFC3339 UTC
    spent        REAL NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);

-- 积分流水：charge / release / adjust / reset
CREATE TABLE IF NOT EXISTS platform_credit_ledger (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform_id TEXT NOT NULL,
    request_id  TEXT,
    model_id    TEXT,
    delta       REAL NOT NULL,
    kind        TEXT NOT NULL,
    note        TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_credit_ledger_platform
    ON platform_credit_ledger(platform_id, created_at);

-- 每条请求最终计入的积分；未计费/失败为 NULL
ALTER TABLE request_stats ADD COLUMN credits REAL;
```

**为什么不用 `request_stats` 推导**：现有 Key 配额是“现存统计里的用量”，清理历史会改变分母；积分是上游账号已经扣掉的额度，历史删掉不能把额度退回来。台账必须独立。

---

## 3. 请求详情：采集与展示

### 3.1 入站路径与请求头

`openai_chat_completions` / `anthropic_messages` 已有 `HttpRequest`，在调用 `handle_request` 前收集：

```rust
struct InboundMeta {
    path: String,
    headers: Option<serde_json::Value>, // 采集关闭时 None
}
```

- `path`：`req.path().to_string()`，包括 `/v1/completions`（它和 chat 共用一个 handler）。
- 请求头白名单（不区分大小写）：
  `content-type`、`accept`、`user-agent`、`x-request-id`、`anthropic-version`、`anthropic-beta`、`openai-organization`、`openai-beta`。
- 强制排除（即使将来加白名单也不收录）：`authorization`、`proxy-authorization`、`x-api-key`、`cookie`、`set-cookie`。
- 同名多值：存成数组；单值存字符串。总大小上限 8KB，超出截断并置 `truncated=1`（只影响请求头展示，不影响重放）。
- 采集开关：复用 `defaults.capture_request_bodies`。关着时 `headers=None`、正文不落库；路径和转发地址仍记录（属元数据）。
- 请求头写入独立表 `request_headers`，**先于正文大小判断**：正文超过 64KB 不存时，请求头仍然保留。
- 重放请求（`replay_request`）没有入站头，传 `None`；不重复存正文（现有行为保留）。

### 3.2 转发地址

`build_forward_request` 返回 `(body, url)` 后，立即调用：

```rust
fn sanitize_forward_url(raw: &str) -> String
```

- 解析 URL；解析失败则返回 `[invalid url]`，不原样落库。
- 把 query 中键名命中 `key`、`api_key`、`apikey`、`token`、`access_token`、`sig`、`signature` 的值替换为 `redacted`。
- 保留其余 query（Azure 的 `api-version`、Anthropic 的 beta 参数等），便于排查。
- 存进本次 `RequestAttempt.forward_url`；每次尝试各自一份。

### 3.3 响应正文

新增设置 `defaults.capture_response_bodies`，默认 `false`，与请求正文开关独立。

- **非流式**：`resp.bytes().await` 之后、解析之前，把原始字节按 64KB 截断后投递 `StatsWriter::record_response_body`。
- **流式透传**：`StreamPassthroughBody` 增加 `capture: Option<CaptureBuf>`；`poll_next` 收到 `Bytes` 时先追加（超过 64KB 停止追加并置 `truncated`），流结束、上游错误、客户端断开（`Drop`）时投递。
- **流式跨协议**：`StreamConvertBody` 同样在 `converter.push_chunk` 之前追加**上游原始字节**；存的是上游 SSE，不是转换后的客户端 SSE。
- **上游非 2xx**：主链路 `resp.text().await` 之后投递响应正文（仍受开关与上限约束）；连接错误没有响应正文。
- 响应记录的 `protocol` 填上游协议（`openai`/`anthropic`），`status_code` 填上游状态；`content_type` 取上游 `content-type`，缺失为 NULL。
- 关闭开关后，新请求不再写；已存记录由“清空已存正文”或保留期删除。

### 3.4 采集上限与重放安全

| 数据 | 上限 | 超限行为 | 影响 |
|---|---|---|---|
| 入站请求正文 | 64KB | **不存** | 重放按钮不可用（现有行为） |
| 入站请求头 | 8KB | 截断并标记 | 只影响展示 |
| 响应正文 | 64KB | 截断并标记 | 只影响展示 |
| 单条错误摘要 | 300 字符 | 截断 | 现有行为 |

请求正文超限仍不存，是为了不让“截断的 JSON”进入重放接口——重放必须逐字还原。

### 3.5 查询接口与前端

新增：

```http
GET /api/stats/requests/{id}/payload
```

返回：

```json
{
  "request": {
    "headers_captured": true,
    "body_captured": true,
    "protocol": "openai",
    "headers": { "content-type": "application/json", "user-agent": "..." },
    "body": "{...}",
    "bytes": 1234,
    "truncated": false
  },
  "response": {
    "captured": true,
    "protocol": "openai",
    "status_code": 200,
    "content_type": "application/json",
    "body": "{...}",
    "bytes": 5678,
    "truncated": false
  }
}
```

- 未采集时对应字段为 `false` 且不返回内容；前端显示原因（请求正文采集关、响应正文采集关、请求早于 V23、超限未存）。请求头与请求正文分别判断，长请求可以只有头、没有正文。
- `GET /api/stats/requests/{id}/trace` 的每次 attempt 增加 `forward_url`。
- `RequestLogItem` 增加 `credits`（B 功能）与 `path`（详情概览用）；列表 SQL 用 `LEFT JOIN` 保持旧数据可读。
- Traffic 详情页新增四个折叠区：
  1. 请求概览（路径、请求 ID、重放来源）
  2. 尝试轨迹（现有表格 + 转发地址列）
  3. 请求头 / 请求正文（现有重放按钮保留）
  4. 响应（状态、Content-Type、正文、截断提示；明确写“上游原始响应”）
- 列表页不加载正文，避免一次 50 行把大字段拖进浏览器。

### 3.6 自动保留期

- 配置新增 `defaults.auto_purge_days: u64`，默认 `0`（关闭）。
- 允许值：`0 / 30 / 90 / 180 / 365`；接口层拒绝 1–29，原因是现有 Key 配额最长窗口 30 天，清理会更早地删掉“本月已用”，导致配额回退。
- 共享维护函数：

```rust
// src/maintenance.rs
pub fn spawn_auto_purge(db: DbPool, config: SharedAppConfig);
```

  - `std::thread::spawn`，启动后先检查一次，之后每 24h 检查；
  - 每轮读取当前配置；`auto_purge_days == 0` 时跳过；
  - 调用 `db::stats::purge_older_than(days, false)`，`tracing::info!` 记录删除行数；
  - 独立服务（`src/main.rs`）与 Tauri（`src-tauri/src/lib.rs`）在 `StatsWriter` 启动后各调用一次。
- 手动清理保留，仍走 `dry_run → 确认`。
- `purge_older_than` 增加 `response_bodies` 的预览与删除；`cleanup_dangling`、`delete_by_proxy`、`delete_all_bodies`、`db_info` 一并覆盖 `response_bodies`。
- 设置页“数据”分组增加：自动清理开关（由保留天数是否为 0 表达）+ 保留窗口 Select；开启时展示“会删掉早于 N 天的请求明细、尝试轨迹、请求正文、响应正文；Key 配额按现存统计计算”的提示。

---

## 4. 积分额度：数据与算法

### 4.1 周期定义

- `credit_window`：`day` / `week` / `month`，NULL = 不启用。
- 校验：`credit_budget` 有值时 `credit_window` 必填（默认 `day`）；`credit_budget` 为 NULL 时 `credit_window` 必须为 NULL。两者不能出现“有预算但没有重置窗口”的半套配置。
- 使用**服务器本地时区**的自然边界：
  - `day`：本地当天 00:00
  - `week`：本地周一 00:00
  - `month`：本地当月 1 日 00:00
- `period_start` 存 UTC RFC3339（与全库时间口径一致）；`reset_at` 由后端按本地时区算出后返回给前端展示。
- 惰性重置：读/写台账时若 `period_start != 当前窗口起点`，先把 `spent` 归零并更新 `period_start`，不需要定时任务。

### 4.2 计费口径

- 计费单位：**每次成功请求**（上游返回 2xx）。
- 单价：`models.credit_price`；NULL = 0（未定价），仍会在平台上标出“未定价模型 N 个”。
- 同一请求内重试同一平台不重复扣；失败尝试释放预留；换到另一个平台则扣新平台。
- 流式请求在**上游 2xx 响应头到达**时确认扣费（此时上游已开始计费）；客户端中途断开不退款。
- 上游 2xx 后解析失败/转换失败仍算已扣费（上游已消耗）。
- 最终失败的请求 `request_stats.credits = NULL`，台账里保留 charge + release 流水（净额为 0）。

### 4.3 预留 → 确认 / 释放

采用“先预留、2xx 确认、失败释放”，避免并发下 250 积分被击穿：

```text
选后端前：
  读取所有启用积分的平台状态（一次 SELECT）
  过滤掉“剩余 < 本次模型单价”的后端
  若候选为空 → 429，消息含最早重置时间

选中后端后：
  try_reserve(platform_id, price)   // SQLite 事务，条件 UPDATE
    成功 → 继续发请求
    失败 → 从候选移除该后端，重新选
  上游 2xx → 保留预留，写 charge 流水，stats.credits = price
  上游非 2xx / 连接错误 → release(platform_id, price)，写 release 流水
```

`try_reserve` 的 SQL 语义（放在 `db::credit`）：

```text
BEGIN IMMEDIATE
  SELECT period_start, spent FROM platform_credit_state WHERE platform_id = ?
  若不存在：INSERT (platform_id, current_period, 0)
  若 period_start != current_period：UPDATE spent = 0, period_start = current_period
  若 spent + price > budget：ROLLBACK，返回 NoCredit
  UPDATE spent = spent + price, updated_at = now
COMMIT
```

- 平台未设置 `credit_budget`：`try_reserve` 直接返回 `NotMetered`，不做任何写。
- `credit_price` 为 NULL：按 0 处理，返回 `Free`，写一条 `credits = 0.0` 的统计。
- `release` 只回退当前周期的预留；若跨周期（请求跨过零点），不把旧周期的额度退进新周期，直接 no-op 并记一条 `release` 流水（note 说明跨周期）。
- 台账清理：`platform_credit_ledger` 保留 180 天，由同一个维护任务顺手清理；`platform_credit_state` 永远只保留一行/平台。

### 4.4 状态与读数

`GET /api/platforms/credits`：

```json
[{
  "platform_id": "p1",
  "platform_name": "ModelScope",
  "budget": 250,
  "window": "day",
  "spent": 42,
  "remaining": 208,
  "period_start": "2026-10-25T16:00:00Z",
  "reset_at": "2026-10-26T16:00:00Z",
  "unpriced_models": 2,
  "exhausted": false
}]
```

- `remaining` 下限为 0。
- `unpriced_models`：该平台启用积分但 `credit_price IS NULL` 的模型数。
- `reset_at`：下一个自然窗口起点（本地时区换算成 UTC 返回）。
- 平台列表/详情只读这一份，不在 `Platform` 结构里塞运行态字段。

### 4.5 手动校准

```http
POST /api/platforms/{id}/credits/adjust
{ "delta": -5, "note": "上游实际扣费与本地差 5" }

POST /api/platforms/{id}/credits/reset
```

- `adjust`：`spent = max(0, spent + delta)`，写 `adjust` 流水；`delta` 不能为 0，限制在 ±1e6。
- `reset`：`spent = 0`，`period_start = 当前窗口起点`，写 `reset` 流水。
- 仅对启用积分的平台可用；未启用返回 400。

### 4.6 告警

在 `src/db/alert.rs` 的扫描器中新增：

| kind | 触发 | 去重键 |
|---|---|---|
| `platform_credit_exhausted` | 剩余 = 0 | `platform_credit_exhausted:<platform_id>:<period_start>` |
| `platform_credit_low` | 剩余 ≤ 20% 且 > 0 | `platform_credit_low:<platform_id>:<period_start>` |

- 告警参数：平台名、已用/总额、重置时间。
- 周期切换后去重键变化，新周期可以重新提醒，不需要手动清已读。
- 前端 i18n 增加对应文案；通知中心现有渲染逻辑直接吃 `code + args`。

---

## 5. 代码落点清单

### 5.1 后端

| 文件 | 改动 |
|---|---|
| `src/db/schema.rs` | V23/V24；`CURRENT_SCHEMA_VERSION=24`；迁移单测 |
| `src/models/stats.rs` | `RequestStat.path/credits`；`RequestAttempt.forward_url`；`RequestHeaders`；`ResponseBody`；`RequestPayload`；`RequestLogItem.path/credits` |
| `src/models/platform.rs` | `Platform.credit_budget/credit_window`；创建/更新请求字段（三态）；校验 |
| `src/models/model.rs` | `Model.credit_price`；创建/更新/批量/CSV 行字段；校验 |
| `src/models/route.rs` | `Backend.credit_price`；`price_pair` 不动 |
| `src/models/config_transfer.rs` | `PlatformRow.credit_*`、`ModelRow.credit_price`；`CONFIG_VERSION=2`，`check` 接受 1/2 |
| `src/db/platform.rs` | 读写新列；更新时持久化积分字段 |
| `src/db/model.rs` | 读写 `credit_price`；批量/CSV 导入；全新模型默认 NULL |
| `src/db/route.rs` | 读取后端时 JOIN `models.credit_price` |
| `src/db/credit.rs`（新） | `period_start` / `state_map` / `try_reserve` / `release` / `adjust` / `reset` / `usage_all` |
| `src/db/stats.rs` | `record` 新列；`record_headers` / `record_response_body`；`payload_for_stat`；purge/cleanup/delete/db_info 覆盖请求头、响应正文（积分流水由 `db::credit` 自己清理） |
| `src/db/alert.rs` | 积分告警扫描 |
| `src/cache.rs` | `StatsWriter` 增加 response body 通道 |
| `src/proxy/handler.rs` | 入站 meta、转发 URL 打码、响应采集、积分预留/释放/确认、候选过滤、429 |
| `src/api/stats.rs` | payload 接口；积分列；导出 |
| `src/api/platform.rs` | 积分字段校验；credits 读数/校准/重置接口 |
| `src/api/model.rs` | 积分单价校验 |
| `src/api/settings.rs` | `capture_response_bodies`、`auto_purge_days` |
| `src/config.rs` | 两个新配置 + TOML 读写 |
| `src/maintenance.rs`（新） | 自动清理 + 流水清理，两个入口调用 |
| `src/main.rs`、`src-tauri/src/lib.rs` | 调用 `maintenance::spawn_auto_purge` |
| `src/api/mod.rs` | 注册新路由 |

### 5.2 前端

| 文件 | 改动 |
|---|---|
| `src/types/stats.ts` | attempt/forward_url、log/credits/path、payload 类型、credit usage 类型 |
| `src/api.ts` | `getRequestPayload`、`getPlatformCredits`、`adjustPlatformCredit`、`resetPlatformCredit` |
| `src/pages/Traffic.tsx` | 积分列、详情折叠区、转发地址列、正文查看 |
| `src/pages/Platforms.tsx` | 创建/编辑表单积分预算 + 窗口；列表积分读数 |
| `src/pages/ProviderDetail.tsx` | 表单积分预算；模型表积分单价列；批量编辑 |
| `src/pages/Models.tsx` | 模型表积分单价列 |
| `src/pages/ModelDetail.tsx` | 积分单价输入 |
| `src/pages/Settings.tsx` | 响应正文开关、自动保留期、清理文案 |
| `src/i18n.ts` | 中英全部新键 |
| `src/app/console.css` | 详情折叠区/积分列的少量样式（尽量复用现有 class） |

---

## 6. API 变更汇总

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/stats/requests/{id}/payload` | 请求/响应正文与请求头 |
| GET | `/api/platforms/credits` | 所有启用积分平台的读数 |
| POST | `/api/platforms/{id}/credits/adjust` | 手动校准 |
| POST | `/api/platforms/{id}/credits/reset` | 重置当前窗口 |
| PUT | `/api/settings` | 新增 `capture_response_bodies`、`auto_purge_days` |
| POST/PUT | `/api/platforms` | 新增 `credit_budget`、`credit_window` |
| POST/PUT | `/api/models` | 新增 `credit_price` |
| GET | `/api/stats/requests` | 返回 `path`、`credits` |
| GET | `/api/stats/requests/{id}/trace` | attempt 返回 `forward_url` |
| GET | `/api/stats/requests/export` | CSV 增加 `path`、`credits` |

---

## 7. 测试与验收

### 7.1 单元测试（`cargo test`）

1. 迁移：V23/V24 后 `CURRENT_SCHEMA_VERSION` 一致；新列/新表存在；旧库升级不丢行。
2. 请求头白名单：Authorization/x-api-key/cookie 永不出现；8KB 截断。
3. 转发 URL 打码：`?key=`、`?api_key=`、`?token=` 被替换；其余 query 保留；非法 URL 不落原文。
4. 响应正文：非流式截断、流式截断标记、清空/按年龄删除/按 proxy 删除/悬挂清理都覆盖 `request_headers` 与 `response_bodies`；超过 64KB 的请求仍能查到请求头。
5. 自动保留期：1–29 拒绝、30/90/180/365 接受、0 关闭；维护函数只删早于边界。
6. 积分周期：本地日/周/月边界；跨周期惰性重置；`reset_at` 正确。
7. 积分预留：余额足够、刚好够、不够、并发两次只成功一次；release 回退；跨周期 release no-op。
8. 积分统计：成功记 `credits`，失败为 NULL；未定价模型记 0；未启用平台不写台账。
9. 配置导入导出：V2 文档带积分字段；V1 旧文档可导入且积分默认关闭；V2 导出能被新程序读回。
10. 告警：低额度、耗尽、周期切换后的去重键。

### 7.2 集成/手工验收

- 用本地 mock 上游（OpenAI JSON + SSE）跑一条完整链路：
  - 创建平台（预算 10）→ 模型（单价 2）→ 虚拟模型 → 请求；
  - 连续 5 次成功 → 平台读数 10/10，第 6 次 429；
  - 调大预算/重置后可继续；
  - 重试换平台：A 失败释放，B 成功只扣 B；
  - 打开响应采集，分别验证非流式/流式/跨协议三种响应都能在详情里看到；
  - 关闭采集后新请求不再写正文，已有正文可清空；
  - 自动保留期设为 30 天，构造一条 31 天前的记录，重启后确认被清掉。
- 回归：`cargo test`（154 个 lib 测试，含新增用例）、`npm --prefix frontend run build`、现有 E2E 至少跑 `test_traffic` 相关与 a11y 关键页（若时间允许跑完整 `python -m pytest e2e -q`）。
- UI：明暗主题、中英双语、axe 0 违规；新增开关和折叠区键盘可达。

### 7.3 验收清单

- [x] 请求详情能看到路径、每次尝试的转发地址/模型、请求头、请求正文、响应正文；
- [x] 敏感头与 URL 凭据不打码就落库的情况为零；
- [x] 自动保留期可配置且不会短于 30 天；
- [x] 平台积分可配置、模型单价可配置、耗尽时 429 并说明重置时间；
- [x] 积分台账不被日志清理影响；
- [x] 配置导出/导入、整库备份还原都包含新字段；
- [x] 版本号 1.4.2、README 更新、Release 工作流正文更新；
- [x] 本地安装目录替换并重启后 `/health`、新页面、新迁移均正常；
- [x] GitHub Actions 四个平台构建成功并生成 v1.4.2 Release。

---

## 8. 二次验证记录（文档审阅时发现并修正的问题）

> 这一节是“先写文档、再按代码核对”的产物；每条都已在上面正文中修正。

| # | 初稿问题 | 核对结果 | 修正 |
|---|---|---|---|
| 1 | 想把响应正文塞进 `request_bodies` | 该表主键是 `request_id`，且是重放入口；混用会让重放/清空/清理三处都要判类型 | 改为独立 `response_bodies` 表 |
| 2 | 想从 `request_stats` 聚合积分余额 | Key 配额就是这样做的，但清理历史会让用量回退；积分是上游已扣额度，不能回退 | 改为独立 `platform_credit_state` + `credit_ledger` |
| 3 | 自动清理直接复用任意天数 | Key 月度配额窗口是 30 天，保留 7 天会把“本月已用”删掉 | 自动清理下限 30 天，接口层拒绝 1–29 |
| 4 | 以为流式响应可以在返回后统一采集 | `handle_stream` / `handle_stream_convert` 返回的是流对象，没有统一出口 | 在两个 `poll_next` 内旁路追加，`Drop` 兜底投递 |
| 5 | 想存“客户端最终收到的响应” | 跨协议转换在流式路径里是逐帧转换，存最终结果要改每条返回分支 | v1 明确存上游原始响应，UI 标注；转换结果不入库 |
| 6 | 以为配置导出会自动带上新字段 | `config_transfer` 的列清单、Row 结构、导入 SQL 都是手写的 | 加入积分字段；`CONFIG_VERSION` 升 2，旧文档仍可导入 |
| 7 | 以为积分可以直接放进 `RouteCache` | 积分余额每个请求都在变，缓存会读到旧值 | 平台预算进缓存，余额走 DB 状态表；每次请求一次快照 |
| 8 | 重试路径可能重复扣费 | 现有重试在 `continue` 前没有统一的释放点 | 每次尝试独立预留；非 2xx/连接错误分支显式 release；2xx 才确认 |
| 9 | 全部平台耗尽时想记一条统计 | `request_stats.backend_id` 非空，且没有选中后端；Key 配额拒绝也不落统计 | 返回 429，不落请求统计；只写日志与告警 |
| 10 | 清空正文只删请求正文 | `delete_all_bodies`、`delete_by_proxy`、`cleanup_dangling`、`purge` 各有一份 SQL | 四处都补 `request_headers` 与 `response_bodies`；`db_info` 增加读数 |
| 10b | 请求头挂在 `request_bodies` 上 | 正文超过 64KB 时该行根本不写，请求头会一起丢失；常见于图片/长上下文请求 | 请求头独立成 `request_headers` 表，先于正文大小判断写入 |
| 11 | 请求头采集想“全存再打码” | 白名单外的新敏感头会漏；打码规则永远追不上 | 白名单收录 + 强制排除清单，默认不存未知头 |
| 12 | 自动清理任务只加在独立服务 | Tauri 走 `src-tauri/src/lib.rs` 自己的 server 启动 | 抽 `maintenance::spawn_auto_purge`，两个入口都调用 |
| 13 | 积分告警想加定时任务 | 现有告警是读时扫描，已有调度语义 | 加到 `alert::list` 的扫描器，不新增线程 |
| 14 | 平台更新沿用 `Option<T>` 无法清空预算 | 现有平台更新用 `Option`，传 null 和“没写”分不清 | 积分字段用 `NullableNumber` / `NullableString` 三态 |
| 15 | 请求正文超限想截断存储 | 截断的 JSON 不能重放，会破坏重放接口 | 请求正文超限仍不存；响应正文才截断 |

---

## 9. 发布与部署步骤

1. **版本号**：`Cargo.toml`、`src-tauri/Cargo.toml`、`frontend/package.json`、`frontend/package-lock.json`（两处 version）、`src-tauri/tauri.conf.json` 全部 1.4.1 → 1.4.2。
2. **README**：中文 README 增加“请求详情/保留期”和“上游积分额度”两节；同步英文 README；其余语言至少同步功能列表与版本相关文案。
3. **Release 工作流**：`.github/workflows/release.yml` 的 `releaseBody` 从 v1.4.1 改为 v1.4.2，加入本次更新内容。
4. **提交**：功能提交按 `feat(request-log)` / `feat(credits)` 分开；最后 `chore(release): v1.4.2`；打 tag `v1.4.2`。
5. **本地打包**：用 `.dsh-tmp/build-tauri.bat`（MSVC + 本地 TEMP）全量 `cargo tauri build --no-bundle`；前端 `npm --prefix frontend run build` 已由 Tauri 构建包含。
6. **本地替换**：按 `tauri-desktop-deploy` 流程停止进程、备份 exe/static/db、替换安装目录与 `_up_` 资源、重启；验证 `/health`、`/api/auth/status`、首页静态资源 hash、迁移到 V24。
7. **GitHub 打包**：推送 `main` 与 tag `v1.4.2`，等待 Release 工作流四个平台全部成功；确认 Release 资产存在。
8. **回滚**：本地保留旧 exe/static/db 备份；GitHub 侧若工作流失败，修复后重新打 tag（或 `workflow_dispatch`），不覆盖已有 tag。

---

## 10. 风险与取舍

| 风险 | 处理 |
|---|---|
| 响应正文采集拖慢流式 | 只在开关开启时复制字节，64KB 上限；追加是内存操作，不做同步 IO |
| 正文表膨胀 | 64KB 上限 + 保留期 + 设置页体积读数 + 一键清空 |
| 积分并发超扣 | 预留用 SQLite 写事务 + 条件 UPDATE；失败释放 |
| 积分与上游实际扣费有偏差 | 手动校准 + 台账流水；明确“以本地记录为准”的提示 |
| 自动清理误删 | 下限 30 天、默认关闭、设置页说明、日志记录删除行数 |
| 旧配置导入丢积分 | `CONFIG_VERSION=2`，旧文档按默认值导入；导出带上新字段 |
| 新增字段破坏旧前端 | 后端字段全部可空/有默认；前端旧版本忽略未知字段 |

---

## 11. 交付顺序

1. 迁移 + 模型/DB 层 + 单元测试；
2. 请求详情采集 + payload API + Traffic 详情 + 保留期 + 维护任务；
3. 积分台账 + 选路 + API + 平台/模型 UI + 告警；
4. 配置导入导出 + 导出 CSV + i18n；
5. 全量测试 → 版本号/README/Release 正文 → 提交 tag → 本地打包替换 → GitHub 打包。
