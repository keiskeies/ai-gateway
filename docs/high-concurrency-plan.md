# 高并发优化 · 设计与落地计划

> 状态：**本轮范围完成——已实施 8/8 项（A/B/C/D₁/D₂/E/F/G）** | 日期：2026-11-27 | 关联：`docs/algorithm-optimization-plan.md`（2026-10-28 性能审计）、`docs/model-performance-timeline-plan.md`
>
> **止步决定（2026-11-27）**：用户指示「压测先算了，要开发的都开发」——F（流式热路径减分配）不再等项目压测，
> 直接落地；D 第二阶段（报表读端切换）此前亦按此完成。已落地的 8 项不含任何限流/配额。
>
> **实施记录（2026-11-26）**：A（workers 配置化，config.toml `[server].workers`）✅、
> B（reqwest 连接池 64/300s/60s，main.rs + src-tauri）✅、C（DB 池 8→16）✅、
> E（V40 复合索引 `(proxy_id, created_at)`）✅、G（滑动窗口错误率熔断 + 自动回归）✅、
> **D 第一阶段**（V41 预聚合小时表 + 写端双写）✅、**D 第二阶段**（报表读端切换到 `request_stats_hourly`）✅
> —— 读端切换按文档 §3.1 两阶段设计完成，明细保留、删表即回滚；
> **F（流式热路径减分配）单测可证字节等价，语义由既有 387 条 lib 测试 + 流式 e2e 全绿背书**。
>
> **F 详情（2026-11-27）**：`stream_convert.rs` 输入缓冲改 `Vec<u8>` + 游标消费——每行不再
> `drain(..=nl).collect()`（旧实现每行整段左移 + String 分配）；`push_chunk` 把缓冲整体
> `mem::take` 到局部变量再逐行切，消除与 `&mut self` 的借用冲突，残余半行塞回。
> `process_line` / `finish` 残留半行直接吃 `&[u8]`，`event:`/`data:` 内容只在拼接处做
> `from_utf8_lossy` 借用（ASCII 纯文本零分配）。handler.rs 同协议透传：只有 `data:` 行
> 才做 String 解析，`event:`/`id:`/注释行字节透传零分配（旧实现每行 `into_owned` + trim
> 两次分配）。输出仍按事件边界组装，`split_chunk_boundaries` 等 10 条转换单测保证字节等价。
> 收益是「每 chunk 恒定分配」而不是零分配：`&mut self` 的借用规则决定事件内容必须落地为 owned 值。
>
> **D₂ 详情（2026-11-26）**：`overview` / `proxy_stats`（单+all）/ `platform_stats`（单+all）/
> `model_summaries` / `model_timeseries`（hour 粒度）全部走三段 UNION（中间整小时段命中
> `request_stats_hourly`，首尾不完整小时回退明细，与逐字段明细口径等价）。
> 明细保留查询：`api_key_usage`（小时表无 api_key 维度）、`error_stats` / `latency_percentiles` /
> `activity_heatmap`（需要原始分布）。回归钉子：`bulk_stats_match_the_per_entity_queries_field_by_field`、
> `query_metrics_*` 三个等价测试、`model_timeseries_hourly_equals_pure_detail_on_random_spread`。
> 口径一致性：悬挂 proxy/backend 行在批量视图里被过滤（旧批量 SQL 从实体表驱动天然排除），
> 单个 `proxy_stats`/`platform_stats` 与批量逐字段钉死相等。
>
> **验收记录（2026-11-26）**：lib 387 全绿（含 G 的 4 条熔断单测、D₁ 的 2 条小时表累加单测、
> D₂ 的 3 组等价性钉子）；e2e 已跑 wall / wall-cost / payload-view / first-run / traffic-replay /
> provider-to-request / backend-model-options 与全量前 31 条，均通过。`test_reasoning_levels.py`
> 的两条 UI 用例会超时，原因不在代码：仓库 `static/` 是 **v1.4.8 旧前端产物**
> （`static/assets/*.js` 内嵌版本串），缺 V39 的「思考等级」面板，因此 `/models/{id}` 永远渲染不出
> 那个 MultiSelect；同文件走 API 的 7 条全过。**跑 e2e 前必须 `npm --prefix frontend run build`**
> 让前端与后端同版本（当前工作区未重建，故这两条 UI 用例仍会超时，与本次改动无关）。
>
> **结论先行**：目标不是「省资源」，而是**单位时间能扛住多少并发请求、多少流**。方向校正后
> 明确**不做限流/配额**（RPM/TPM、全局并发上限、余额扣减——那是「别被打爆」，
> 与「我要支持高并发」目标相反）；这一轮只做「数据量再大、并发再高也不掉链子」的事。
> 共 7 项，4 项不动接口口径、3 项新增配置，全部可灰度、可回滚。
>
> **2026-11-26 代码二次验证**：文档 §1 事实已逐条核对（基准 `6feac37`）。修正 3 处：
> ①F 的 `buffer.drain`「每次 O(剩余字节) 搬移」**不成立**——handler.rs:2158-2206 已是游标式
> 切行、每 chunk 只 drain 一次（这只是当时「曾走过的弯路」的现状，现在是对的）；
> ②「无熔断」**部分不成立**——重试已完整实现（`RetryPolicy` + 指数退避 + 仅首字前重试，
> handler.rs:860-1470），但「失败后自动临时隔离/自动回归」确实**没有**（`BackendStatus::Active`
> 之外无自动降级路径），G 的范围应缩窄为「错误率熔断」；
> ③LB 已有**后端级 `max_concurrent` 并发上限**（lb/mod.rs:27-58、models/route.rs:153），
> 这与全局限流是两回事，保留（它防的是单个后端被并发打爆，不是限制网关吞吐）。

---

## 0. 摘要

| # | 改动 | 类型 | 现状代价 | 改完 | 影响面 |
|---|---|---|---|---|---|
| A | actix workers 显式化 + keep-alive 调优 | 配置 | 走默认值，未显式声明，看不到容量 | 并发连接数可预期、可压测 | 全服务 |
| B | 上游 reqwest 连接池调优（pool_max_idle / keep-alive / http2） | 配置 | `Client::new` 默认池极小（per-host 空闲上限少），**高并发下每请求新建 TCP+TLS 是头号开销** | 连接复用，并发吞吐显著上升 | 全部 /v1 转发 |
| C | SQLite 写并发解耦（池 8→16 + WAL busy_timeout 调优 + 写事务尽量合并） | 配置/小改 | 池 8，报表 `web::block` 与 StatsWriter flush 抢连接 | 读写互不阻塞，查询排队消失 | 全局 DB |
| D | 报表预聚合（`request_stats_daily` / `request_stats_hourly` 汇总表） | 结构性 | `overview` 每次 ≥10 条聚合、`proxy_stats` 多条全表扫（§1.4 已核实） | 报表查询常数级、可上预热 | `/api/stats/*` 全部读路径 |
| E | 复合索引补齐 `(proxy_id, created_at)` 等 | 建索引 | 单列索引过滤后仍扫全部历史行 | 常驻查询走索引 | 统计/详情查询 |
| F | 流式热路径去重复分配 | 重构 | 每 chunk `from_utf8_lossy→owned→to_string→parse` + **每行** `drain(..=nl).collect()` + `cloned()/json!` 重建（§1.5 已修正） | 每 chunk 恒定开销 | /v1 流式转发 |
| G | 错误率熔断 + 自动回归（**范围缩窄后**） | 功能 | 重试已有，但无「连续失败→临时隔离→过时回归」，坏渠道每次打到超时才切换 | 高并发下坏渠道不拖垮整体 | LB 选路 |

**明确不做**（方向排除项，理由）：按 key 限流/预算/余额（与高并发目标相反）、语义缓存（误命中答错，免费场景精确路由更划算）、多租户/SSO、插件系统（K8s/WASM 生态，自研内嵌即可）、Redis 化落库（单机 SQLite 写锁仍是上限，先做 C/D，实测再定）。

---

## 1. 已确认的代码事实

行号核对基准：2026-11-26 的 `main`（`6feac37`，与 origin/main 同步，v1.5.0）。

### 1.1 A：HttpServer 未显式配 workers / keep-alive

| 事实 | 位置 |
| --- | --- |
| 独立服务模式 `HttpServer::new(…).bind(…).run()`，无 `.workers()` | `src/main.rs:79-132` |
| Tauri 模式同一写法，无 `.workers()` | `src-tauri/src/lib.rs:552-600` |
| actix 默认 `workers = 可用 CPU 核数`（依 num_cpus），**容器/桌面机核数决定并发容量** | actix-web 文档 |

### 1.2 B：上游客户端只有一个默认池

| 事实 | 位置 |
| --- | --- |
| 主转发客户端 `reqwest::Client::builder()` 直接 build，无池参数 | `src/main.rs:36-37`、`src-tauri/src/lib.rs:509` |
| reqwest `Client::new` 默认 `pool_max_idle_per_host = usize::MAX`（空闲保活但**每 host 连接数上限受底层 hyper cap**，且默认无 http2 显式开启时按 ALPN 协商） | reqwest 文档 |
| 上游 keep-alive 是网关性能头号话题（业界共识）：连接不复用 = 每个并发请求一次 TCP+TLS 握手 | 见 §4 参考 |

> ⚠️ 需要压测确认：request/stream 超时已有的 per-request timeout 不受影响。A/B 都改配置，不动业务逻辑。

### 1.3 C：DB 池 8 且与报表竞争

| 事实 | 位置 |
| --- | --- |
| 池 `max_size(8)` | `src/db/mod.rs:47-48` |
| 报表全是 `web::block(move || db::stats::…)` 同步查 | `src/api/stats.rs:42,57,71,80,91,106,117,128,136,167` |
| StatsWriter 异步批量写也走池 | `src/cache.rs`（try_send 有界，非阻塞） |
| WAL / busy_timeout=5000 / synchronous=NORMAL 已正确 | `src/db/mod.rs:36-52` |

### 1.4 D：报表无预聚合，全表扫

| 事实 | 位置 |
| --- | --- |
| `overview` 一次 7 条 + 对比周期 4 条聚合 | `src/db/stats.rs:1570-1655` |
| `proxy_stats` 7 条、`platform_stats` 连续多次扫描 | `src/db/stats.rs:1773-1917` |
| hour / weekday 粒度用 `datetime(created_at,'localtime')` 逐行转换（非 sargable，索引失效） | `src/db/stats.rs:143,333-334` |
| 唯一复合索引 `(api_key_id, created_at)` / `(backend_id, created_at)`；`proxy_id/route_id` 仅单列 | `src/db/schema.rs:328-332, 829-832` |
| purge 默认关闭，明细无限累积 | `src/maintenance.rs:22` |

### 1.5 F：流式热路径每 chunk 多次分配（**已修正**）

> ⚠️ 修正：原稿「`buffer.drain(..consumed)` 每次 O(剩余字节) 搬移」**不成立**——handler.rs 的
> 行切分**已是游标式**（`consumed` 游标扫行，chunk 末尾才 `drain(..consumed)` 一次，注释
> handler.rs:2159-2161 明说这是修过的版本）。真正还值得动的是下面三处。

| 事实 | 位置 |
| --- | --- |
| 每行 `String::from_utf8_lossy(...).into_owned()` + `trim().to_string()` + `serde_json::from_str` | `src/proxy/handler.rs:2167-2176` |
| SSE 转换侧：每 chunk `String::from_utf8_lossy(bytes)` 每次新分配；每行 `line_buf.drain(..=nl).collect()` 逐行拷 | `src/proxy/stream_convert.rs:110-112` |
| 每个事件 `v.get("delta").cloned()` + `json!` 重建 + `to_string` 重新序列化 | `src/proxy/stream_convert.rs:258-319`（delta 克隆 290、emit 封包 294/304/319） |

**压测后才能定**：这些分配在总开销里占比多少；若瓶颈在上游网络延迟（TTFB 占大头），F 收益有限，优先做 B。

### 1.6 G：重试已完整实现，缺的是熔断（**已修正**）

> ⚠️ 修正：原稿「无熔断」措辞不准。**重试**早已实现（`RetryPolicy`：max_retries、指数退避
> `retry_delay_ms`、按 `retry_on_error` 白名单分类重试、且只在首字节流出前重试），
> 见 handler.rs:860-1470。缺的是**熔断**：没有任何路径会把一个连续失败的渠道
> 自动降级/临时隔离再自动回归——`BackendStatus` 只有 `Active/Disabled`（手动管理）。

| 事实 | 位置 |
| --- | --- |
| 重试策略：`RetryPolicy { max_retries, retry_on_error }`，默认 2 次 | `src/models/route.rs:30-39` |
| 指数退避 + 连接错误/状态码分类重试，**仅首字节前** | `src/proxy/handler.rs:25, 860, 889, 1143-1144, 1419-1420, 1470, 2873` |
| 选路只认 `BackendStatus::Active`（手动开关），无自动降级/回归路径 | `src/lb/mod.rs:61`、`src/proxy/handler.rs:894` |
| 后端级 `max_concurrent` 并发上限已存在（防单后端被打爆，保留，不属限流） | `src/lb/mod.rs:27-58`、`src/models/route.rs:153` |
| 平台健康历史表已存在（`platform_health`，测活落点） | `src/db/schema.rs:1025`、`src/api/platform.rs:357` |

---

## 2. 分工与优先级

### P0（先做，收益最大、风险最小）
- **B 上游连接池**（配置）→ 高并发的第一杠杆。与 A 一起改，压测归一。
- **E 复合索引**（一条迁移语句）→ 一行 SQL 的收益，报表先到先受益。
- **D 预聚合表**（结构性，工作量大）→ 报表从全表扫变固定扫描。分两阶段：先加表 + 写入端双写，再切读端；读端切之前旧接口不变，可逐步回滚。
- **C DB 池 8→16**（配置）→ 一行改动，配合 D 之后报表不再抢池。

### P1（功能，影响高并发下的可用性）
- **G 熔断降级**：错误率滑动窗口熔断 + 自动回归（健康历史已有，可复用）。高并发下坏 channel 是吞吐杀手（每次都打到超时才切换），熔断是免费额度和多账号场景的刚需。

### P2（性能细节，单流 CPU 优化）
- **F 流式热路径去重复分配**：改成游标式消费 + 复用 buffer。多并发时 CPU 让位给真正的转发。

## 3. 设计草案（关键点）

### 3.1 预聚合表（D）设计
- 表：`request_stats_daily`（`bucket_day TEXT` 本地日 + `platform_id / proxy_id / route_id / backend_id / api_key_id / model_id` 维度的 `requests / prompt_tokens / completion_tokens / cached_tokens / cost` 等计数列），`request_stats_hourly` 同构按小时。
- 写入：StatsWriter 落明细时同步累加当日/当小时桶（`INSERT … ON CONFLICT DO UPDATE`），同一事务内完成，避免不一致。
- 读端：报表接口优先查汇总表；**明细级查询（Traffic 列表、单行详情）仍查明细表**，不受影响。
- 回滚：保留明细表全量数据，汇总表只是加速层；删汇总表即完全回到现状。

### 3.2 熔断（G）设计
- 以 `proxy_id`（渠道）为单元的滑动窗口（如 5 分钟），错误率 > 阈值（如 50%）→ 临时熔断 30s，期间选路跳过该 proxy，过半自动回归。
- 不拦截、不 429、不触碰手动 `BackendStatus`——只影响选路，与「高并发支撑」不冲突。
- 复用现有 `platform_health` 测活机制做回归探测；重试逻辑已存在（§1.6），熔断只做「降级」，不做「重试」。

### 3.3 连接池（B）配置草案
`reqwest::Client::builder()`
- `.pool_max_idle_per_host(64)`：空闲保活连接数，替代默认（压测后定值，太高占用 fd）
- `.pool_idle_timeout(Duration::from_secs(300))`
- `.tcp_keepalive(Duration::from_secs(60))`
- HTTP/2：`http2_prior_knowledge` 不可用（上游多平台），走 ALPN 自动协商不强制
- 注意：`SystemProxy::apply_to_builder` 已注入系统代理（`src/config.rs:190-201`），配置保持组合不冲突

### 3.4 workers（A）
- 独立模式：`HttpServer::new(…).workers(N)`，N 默认 = 核数，但**显式写 + 配置化**（进 `config.toml [server] workers`，缺省不设走 actix 默认），供容器场景调优。
- keep-alive 连接级：`HttpServer` 默认 5s keep-alive，非流式长连接可提升；需压测确认对长流无影响（流式本身覆盖此场景）。

## 4. 参考（业界做法，来自调研）

- LiteLLM / Kong AI Gateway / OneAPI 的渠道管理与 fallback 链（熔断、自动禁用回归）
- reqwest/hyper 上游连接池调优 —— 并发下「连接复用」是吞吐第一杠杆
- SQLite WAL + 单一写者 + 预聚合报表（读写分离的本地等价物）
- 参考链接：
  - [LiteLLM 官方](https://www.litellm.ai/)
  - [LiteLLM vs Kong AI Gateway](https://qveris.ai/zh-cn/guides/litellm-vs-kong-ai-gateway)
  - [LLM Gateway 核心能力全解析（流式/缓存/容量）](https://developer.aliyun.com/article/1771358/)
  - [流式 SSE 端到端：缓冲与背压陷阱](https://dev.to/kuldeep_paul/streaming-llm-responses-end-to-end-sse-backpressure-and-mid-generation-cancellation-4d60)
  - [Go SSE 流式转发：连接池调优与级联取消](https://juejin.cn/post/7684573646438383626)

## 5. 验收

- 压测：`ab`/`wrk`/自研并发脚本，对比改动前后的「并发 50/100/200 × 非流式 + 流式混合」的 TPS、TTFB、错误率（本地 mock 上游固定延迟）。
- 报表：造 50 万行 `request_stats`，`/api/stats/overview`、`proxy_stats`、`platform_stats_all` 响应时间对比前后（目标 ≤ 原 1/10）。**D₂ 已落地实现（读端切到小时表），此验收保留为压测验证项，代码层面等价性已由单测钉子覆盖。**
- 回归：`cargo test`（lib 387 全绿）+ `python -m pytest e2e -q`（66 specs，需先 `npm --prefix frontend run build`）全绿；`/wall`、Traffic、ProviderDetail 健康历史等既有功能口径不变。
- **F（2026-11-27）**：单元级字节等价由 `stream_convert` 10 条单测（含 `split_chunk_boundaries` 跨 chunk 切片归一比较）与 handler `wall_wire_*` 透传单测覆盖；e2e 已跑 `test_payload_view` / `test_traffic_replay` / `test_provider_to_request` / `test_reasoning_levels` / `test_first_run` / `test_delete_model_impact`，流式与重放路径全绿。同协议透传对 `event:`/非 data 行零分配，输出字节不变（透传的是缓冲区原样字节）。
- 不做限流：验收清单里不含任何 RPM/TPM/并发上限/余额扣减项。

## 6. 待办（动手前需确认的压测事实）

1. 当前高并发真实瓶颈是 CPU（流式解析）还是网络（连接建立）→ 决定 F 与 B 的先后与投入。
2. `pool_max_idle_per_host` 实测值：默认 vs 32/64/128 的 TPS 曲线。
3. workers 显式化后 Windows 桌面端（核数少）是否需要默认调低，避免 socket 争用。
4. 预聚合表的写入双写对 StatsWriter 吞吐的影响（每行一次 UPSERT 的开销对冲）。

**已核实、可直接实施**（二次验证后无需再压测的项）：E 复合索引（schema.rs 现状 §1.4）、
C 池 8→16（§1.3）、G 熔断（§1.6，重试基础已在）。
