# 算法与数据结构层优化 · 设计与落地计划

> 状态：**已实现并验证**（验收结果见 §9）| 日期：2026-11-20 | 关联：`docs/model-performance-timeline-plan.md`（`/wall` 单模型分时）、2026-10-28 性能审计
>
> **结论先行**：这一轮只动四处，全部是「同样的输入产生同样的输出，但少走冤枉路」。
> 不新增接口、不改任何口径、不改任何读数。其中两项是真正的算法阶数下降
> （O(实体数 × 请求) → O(1) 请求、O(n²) → O(n)），一项是纯 IO 批处理，
> 一项是实测出来的**冗余索引**（与算法无关，但同一轮顺手做掉）。

---

## 0. 摘要

| # | 改动 | 类型 | 现在的代价 | 改完 | 影响面 |
|---|---|---|---|---|---|
| A | `/wall` 改用批量统计接口 | N+1 → 2 次请求 | 每轮 `11 + P + Q` 个 HTTP（实测），逐个那条还走不到索引 | 每轮固定 13 个 HTTP（其中 2 个是批量统计），实测无一个带实体 id 的统计请求 | 只影响大屏取数 |
| B | `ModelTimeline` 的 `sparseDot` | O(n²) → O(n) | 每次 hover 重渲染 `5 × n²` 次比较（n=168 时 ≈1.4e5） | `5 × n` | 只影响单模型分时图 |
| C | `alert::sweep` 包事务 | 逐语句写锁 → 一次提交 | 每轮 `2k+1` 次独立写锁获取（k = 候选告警数，上限 60），实测 1.5 ms/轮 | 1 次提交，实测 0.27 ms/轮（≈5.6×） | 通知中心 / `/wall` 预算条 |
| D | 删两个前缀冗余索引 | 写放大 + 磁盘 | 每行多维护 2 个 B 树；实测每行写慢 ~23% | 查询计划逐条不变 | 全库写入路径 |

`P` = 虚拟模型数，`Q` = 平台数。当前实现随实体数线性膨胀，改完是常数。

**明确不做**（这一轮拒绝，理由在 §6）：配额计数增量表、格式化函数记忆化、滚动高亮 rAF 节流、
前端 SSE 解析「优化」、负载均衡选路优化。

---

## 1. 已确认的代码事实

先列证据，再谈改动。所有行号在 2026-11-20 的 `main`（`92b9871`）上核对过。

### 1.1 A：大屏的 N+1

| 事实 | 位置 |
| --- | --- |
| 大屏逐个拉每个虚拟模型的统计 | `frontend/src/pages/Dashboard.tsx:608-615` |
| 大屏逐个拉每个平台的统计 | `Dashboard.tsx:617-624` |
| 批量接口的前端封装**早就存在** | `frontend/src/api.ts:261` `getAllProxyStats`、`:264` `getAllPlatformStats` |
| 批量接口的后端实现**早就存在** | `src/api/stats.rs:77` `proxy_stats_all`、`:88` `platform_stats_all`；路由 `src/api/mod.rs:93,96` |
| 别的页面**已经在用**批量版 | `frontend/src/pages/Models.tsx:53`、`frontend/src/pages/Platforms.tsx:122` |
| 后端注释自己写明批量是为了替代逐个查 | `src/db/stats.rs:1850`「两条 SQL，替代按 proxy 逐个查询（每个 proxy 原本 6 条）」、`api.ts:263`「每个平台原本 5 条 SQL」 |

**只有大屏没换。** 逐个那条的 SQL 形状（`src/db/stats.rs:1981`）：

```rust
let base = "COALESCE(b.platform_id, 'unknown') = ?1";
```

列被表达式包住，JOIN 侧走不到索引。实测查询计划（SQLite，10 万行样本库）：

```
单平台： SEARCH rs USING INDEX idx_stats_created (created_at>?)        ← 只按时间范围扫
批量：  SEARCH rs USING COVERING INDEX idx_stats_backend_created (backend_id=? AND created_at>?)
```

也就是说 `today` 之外的时段（24h / 7d / 30d），单个平台那 6 条 SQL 每条都要扫整个时间窗，
乘 Q 乘以 6。**A 的收益同时来自「请求数变常数」和「查询计划从范围扫变成点查」。**

### 1.2 A 的约束：失败必须仍然点名

大屏的硬约定（`agents.md` + `Dashboard.tsx:574` 注释）：
**「取不到就记下是哪一块」，`0` 和「这块没取到」在大屏上必须分得开。**

逐个查时，单个实体失败会 `note('healthBoard')` / `note('platformShare')`。改批量后一次失败就是整块失败，
语义**变强**而不是变弱（原来只要漏一个就点名，现在整块取不到更该点名），所以约定仍然成立。
但必须核掉一个隐患：`healthBoard` 除了读数条（`Dashboard.tsx:1289,1291`）以外**没有** `PanelFailed` 分支，
而 `platformShare`（`:1534`）、热力图（`:1504`）、矩阵（`:1633`）、明细（`:1800`）都有。
`proxies` 这个 state 在 `Dashboard.tsx` 里只被用了两处（`:948` 算活跃数、`:1289` 算分母），
批量失败 → `proxies` 变空数组 → `activeProxies` 变 `0`、分母退回 `stats.total_proxies`。
**读数条那一侧是安全的（已经 `'—'`），但读数的语义要从「活跃网关数」改成「有流量的网关数」才能和批量结果自洽**，见 §3.1 第 3 点。

### 1.3 A 的字段核对

大屏声明的 `ProxyWithStats` / `PlatformWithStats`（`Dashboard.tsx:101-129`）只需要
`id` / `name`（`platform_type` / `base_url` 声明了但全文没用过，`platform_type` 出现在 `:118`，无引用）。
批量接口返回的 `ProxyStats`（`src/db/stats.rs:1879-1893`）/ `PlatformStats`（`:1946-1960`）
含 `total_requests` / `success_rate` / `avg_latency_ms` / `total_token_input` / `total_token_output` /
`total_cost` / `priced_requests`，**够用**。

唯一差异：`platform_stats_all` 不返回 `platform_name`（只在 SQL 里 `GROUP BY p.name`、没进 SELECT 列），
但大屏是从 `listPlatforms()` 拿名字、按 `platform_id` 合并统计，因此**不需要**它。
`proxy_stats_all` 的 `requests_today` 在批量版里被 `stats.rs:1900-1910` 单独补齐，大屏也没用这个字段。

### 1.4 B：`sparseDot` 的 O(n²)

| 事实 | 位置 |
| --- | --- |
| `sparseDot` 在组件体内定义，每次渲染都返回新闭包 | `frontend/src/components/ModelTimeline.tsx:198-206` |
| 内部对 `buckets` 做 `findIndex` 线性查找 | `ModelTimeline.tsx:201` |
| 被 5 张图的 `dot` prop 各用一次 | `:393 :416 :417 :433 :475` |
| hover 会触发整片重渲染 | `:358-360` `onMouseMove` → `setHoveredPeriod`；`:384 :411 :428 :452 :470` 五张图共享 |
| `buckets` 只有 2 个成员被读 | `:202-203` 只读 `buckets[index-1]` / `buckets[index+1]`，且 `[key]` 的 `key` 每图固定 |

Recharts 2.15 的 dot 回调**自带 `index`**（已核对 `frontend/node_modules/recharts/es6/cartesian/Line.js:194-205`：
`dotProps` 里有 `index: i`、`payload: entry.payload`）。所以 `findIndex` 是纯浪费——索引是免费送的。

真实上限：`RANGE_GRANULARITIES`（`Dashboard.tsx:238-245`）里只有 `7d` 允许 `hour`，n = 168。
`5 × 168 × 168 ≈ 1.4e5` 次比较 / 每次 hover 重渲染。**不致命，但修复是零风险的删代码**，
而且这个文件被 `e2e/test_model_timeline.py` 覆盖，改完能被现有断言保护。

### 1.5 C：`alert::sweep` 没有事务

| 事实 | 位置 |
| --- | --- |
| `sweep` 拿裸 `conn`，循环里逐条 `query_row` + `execute` | `src/db/alert.rs:409-455` |
| 收尾的「标解决」循环同样逐条 `execute` | `:458-471` |
| 保留期 `DELETE`、`unread` 的 `COUNT` 也在同一段裸写 | `:473-483` |
| 唯一的调用方是 `/api/alerts`，且已经在 `web::block` 里 | `src/api/alert.rs:29-30` |
| 大屏每 30 s 打一次 `/api/alerts` | `Dashboard.tsx:597`（`listAlerts`）+ `:727-735` 自动刷新 |

这与 2026-10-28 审计定位的「逐行 autocommit」是**同一个模式**：`sweep` 每轮至少 `2k + 3` 次
独立写锁获取（k ≤ 60）。但**这笔账不能记成 fsync**：本机是 WAL + `synchronous=NORMAL`，
WAL 提交只做 `wal_flush` 不做 fsync，checkpoint 才 fsync。实测（60 候选、每轮刷新半数）
逐语句 1.5 ms/轮 → 事务 0.27 ms/轮，约 **5.6×**——省的是每条语句一次写锁获取 + WAL 帧收尾
的固定开销，不是 2k 次 fsync。当初按 fsync 估的量级是错的，实际收益也小一个数量级。

嵌套风险已核：`sweep` 内部只有裸 `conn.execute`，`collect_candidates` / `list` 在调用方
`api/alert.rs:29-31` 是 `sweep` **之外**的独立函数，各自 `pool.get()`。
因此在 `sweep` 内开事务**不会**与任何外层事务嵌套。

> **事务行为必须是 `Immediate`，不是默认的 `Deferred`**——这是实施时引入、又被实测抓出来的
> 回归，细节与钉子在 §8 第 6 轮。

### 1.6 D：两个前缀冗余索引（实测发现）

| 索引 | 定义 | 位置 |
|---|---|---|
| `idx_stats_api_key` | `(api_key_id)` | `src/db/schema.rs:615` |
| `idx_request_stats_api_key` | `(api_key_id, created_at)` | `:983` |
| `idx_stats_backend` | `(backend_id)` | `:805` |
| `idx_stats_backend_created` | `(backend_id, created_at)` | `:329` |

SQLite 里 `(a)` 是 `(a,b)` 的**前缀**，前者能被后者完全替代。用 `EXPLAIN QUERY PLAN` 逐条验证
（10 万行样本，先建全 8 个索引，再 `DROP` 掉两个冗余的，对比计划）：

```
                          删之前                                                  删之后
backend_id           COVERING idx_stats_backend_created (backend_id=?)      完全相同
api_key_id           COVERING idx_request_stats_api_key (api_key_id=?)      完全相同
backend_id+created_at idx_stats_backend_created (backend_id=? AND created_at>?) 完全相同
```

**优化器从来没选中过这两个冗余索引**——它们只贡献写放大和磁盘。实测代价：

| 指标 | 8 个索引（现状） | 6 个索引 | 差 |
|---|---|---|---|
| 100k 行样本库文件大小 | 26.23 MB | 23.95 MB | **−8.7%** |
| 30k 行批量提交（模拟 StatsWriter flush） | 536 ms（17.87 µs/行） | 412 ms（13.74 µs/行） | **−23%** |

按审计测到的 ~1.5 GB/天正文 + 每请求一行统计来算，写路径上每行省 2 个 B 树的维护是持续收益。
`request_stats` 是写得多、读得浅的表，正是最不该养冗余索引的地方。

**注意 `idx_stats_created`（单列 `created_at`，`:806`）不在删除范围**：它不是任何索引的前缀，
`stats.rs:141` 的注释和 `latency_percentiles`/`activity_heatmap` 的无 JOIN 查询依赖它（见 §1.5 的
`SEARCH rs USING INDEX idx_stats_created`），删了会退化成全表扫。

---

## 2. 实测基线（用于改后对比）

| 测什么 | 怎么测 | 记录 |
|---|---|---|
| A：大屏一轮的请求数 | Playwright `page.on('request')` 只数 `xhr`/`fetch`，取 `urlparse(r.url).path` | 实测改前 `11+P+Q`（一轮 12 个数据请求里 P+Q 个带实体 id），改后固定 13 个、0 个带 id |
| D：写放大 | `python -m sqlite3` 造样本库，`EXPLAIN QUERY PLAN` 断计划落点不变 + 文件大小 | 见 §1.6 表 |
| C：每轮耗时 | 临时把 `transaction()` 注释掉跑同一组候选，对比 ms/轮 | 1.5 → 0.27 ms/轮（5.6×）；fsync 次数本机测不了，已放弃该口径 |

> 审计测过（2026-10-28）：所有控制台查询 < 35 ms、控制台首屏 FCP 184 ms。
> 所以**这一轮的验收标准不是「变快多少」，而是「行为逐像素不变 + 请求数/复杂度按上表下降」**。
> 任何读数变化都算 bug，包括 `—` 和 `0` 的互相转换。

---

## 3. 改法

### 3.1 A：`/wall` 改用批量统计

改 `Dashboard.tsx:loadData`（`:582-631`）。

1. **并入首轮 `Promise.all`**（`:590-598`），和 `listProxies` / `listPlatforms` 同批发出：

   ```ts
   getAllProxyStats(range).catch(() => { note('healthBoard'); return null }),
   getAllPlatformStats(range).catch(() => { note('platformShare'); return null }),
   ```

   返回 `null`（而不是 `[]`）是关键：**`[]` 是合法的「一个都没配统计」，`null` 才是「没取到」**。
   用 `null` 才能区分「真的没流量」和「这块挂了」，这正是大屏那条约定要的东西。

2. **删掉 `:608-624` 两个 `Promise.all` 逐个查的循环**，改成按 id 建 `Map` 合并——
   直接照抄 `Models.tsx:62-70` / `Platforms.tsx:146-172` 已有的写法（同一份代码形状，不再发明第三种）。
   取不到时 `setProxies([])` / `setPlatforms([])`，`note(...)` 已经在 `.catch` 里点过名。

3. **`activeProxies` 的口径**（`:948`）保持 `p.stats?.total_requests > 0` 的字面判定不变，
   但因为取不到时 `proxies` 为空 → `activeProxies = 0`，所以读数条 `:1289` 必须继续由
   `failed('healthBoard')` 兜住显示 `—`（现在已经是）。**读数条的判定顺序不能动。**

4. **健康看板没有独立面板**（实施时已确认）：`healthBoard` 只存在于读数条 `:1286-1288`，
   而那里已经是 `kpiDown || failed('healthBoard') ? '—' : …`。所以 §2 第 2 轮预留的
   「补 `PanelFailed` 分支」这一步**按预留的分支跳过**——本轮不改任何渲染分支。

5. `getProxyStats` / `getPlatformStats` 的导入（`:50`）**要留着**：
   `ModelDetail.tsx:234`、`ProviderDetail.tsx:314` 的详情页仍然逐个查（单实体，合理）。
   接口本身不删。

**为什么不影响行为**：同一份 SQL、同一份 `RangeQuery`、同一套口径；`proxy_stats_all` 与
`proxy_stats` 用的是同一个 `TimeRange::and()`（`stats.rs:1871` vs `:1786`）。
唯一可观测差异是取不到时的失败粒度从「某个实体」变成「整块」——这是**更保守**的方向。

### 3.2 B：`sparseDot` 用 Recharts 给的 `index`

改 `ModelTimeline.tsx:198-206`，索引直接取 `props.index`。

> **本节初稿写的是 `useCallback`，那是错的，已删。** `sparseDot(key, color)` 是在 JSX 里
> **被调用**的工厂：每渲染都会跑一次 `sparseDot('counts', …)` 并产出一个新闭包，
> 把工厂本身 `useCallback` 起来稳定的只是那个毫无意义的工厂标识，子组件照样拿到新函数。
> 真正的收益只有一处——去掉每点一次的 `findIndex`，把 O(n²) 变成 O(n)。
> 既然 `buckets` 在同一个渲染闭包里，也不必 memo。

实际实现（与代码一致）：

```tsx
// index 由 Recharts 在 dot 回调上给出（recharts/es6/cartesian/Line.js 的 dotProps.index），
// 且五张图都是 data={buckets}，points 与 buckets 逐下标对齐——不必拿 period 反查。
const sparseDot = (key: keyof Bucket, color: string) => (props: any) => {
  const { cx, cy, index, payload } = props
  if (!payload || payload[key] == null || cx == null || cy == null) return <g />
  const prev = index > 0 ? buckets[index - 1][key] : null
  const next = index >= 0 && index < buckets.length - 1 ? buckets[index + 1][key] : null
  if (prev != null || next != null) return <g />
  return <circle cx={cx} cy={cy} r={2.5} fill={color} />
}
```

相对原实现的**唯一**改动：多解构一个 `index`、删掉那行 `buckets.findIndex(...)`。
空值守卫和边界表达式**逐字保留**，不引入 `buckets[index] ?? payload` 这类兜底——
`payload` 本身就是判空对象，再兜一层是给不可能的分支写代码。

等价性论证（必须逐条对上，否则「孤立点」的显示会漂）：

| 原实现 | 新实现 |
|---|---|
| `!payload \|\| payload[key] == null` → `<g />` | 原样保留。`payload` 就是 `points[i].payload`，而 `points` 由 `data={buckets}` 逐下标生成，故 `payload === buckets[index]` |
| `cx == null \|\| cy == null` → `<g />` | 原样保留 |
| `index > 0 ? buckets[index-1][key] : null` | 逐字相同，只是 `index` 改成传入值 |
| `index >= 0 && index < len-1 ? buckets[index+1][key] : null` | 逐字相同 |
| `findIndex` 找不到 → `index = -1` → 前后都 `null` → **画点** | 走不到：`index` 由 Recharts 传入，必然有效。这一行的差异**只可能**出现在原来 `findIndex` 失败的场合，而那意味着 `payload.period` 不在 `buckets` 里——`test_model_timeline.py` 的 seed 若出现这种数据，改前改后都会画点，改后不会少画 |


`buckets` 与 `data` 同源（`:191` `buildBuckets` 的结果直接喂给 5 张图的 `data={buckets}`），
所以 `index` 与 `payload` 不可能错配。**若有人日后把 `data` 换成过滤后的数组，这个假设会破**——
这是本次改动引入的唯一隐含前提，所以要用注释钉住，并把 `payload.period === point.period` 的
一致性交给 `e2e/test_model_timeline.py` 的既有断言（它专门 seed 了 0 请求桶来触发孤立点分支）。

### 3.3 C：`alert::sweep` 包事务

改 `src/db/alert.rs:409`。`sweep` 全部写操作收进一个事务；读的部分（末尾 `unread` 的 `COUNT`）
留在事务外，因为它是只读且要看到提交后的状态：

```rust
let mut conn = pool.get()...;
let now = now_iso();
let mut opened = 0usize;
let mut resolved = 0usize;
let unread;
{
    // IMMEDIATE，不是默认的 DEFERRED —— 见下面的「事务行为」要点
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // 原有的候选循环 + 「标解决」循环 + 保留期 DELETE，原样搬进来（SQL 一字不改）
    ...
    tx.commit()?;
}
unread = conn.query_row("SELECT COUNT(*) ...", [], |r| r.get(0))?;
```

要点：
- **事务行为必须是 `Immediate`。** `conn.transaction()` 是 `BEGIN DEFERRED`：先拿读快照、
  第一条写语句时才升级成写事务，而 **SQLite 在升级时不调用 busy handler**——冲突直接返回
  `SQLITE_BUSY_SNAPSHOT`，`busy_timeout=5000` 形同不存在。本机 WAL 实测：并发写者一提交，
  DEFERRED 的升级 **16 µs** 就失败；IMMEDIATE 同场景等 **259 ms** 后成功。改前的逐语句
  autocommit 每条自己取写锁、走 busy_timeout，所以 5 s 内必然成功——换成 DEFERRED 事务等于
  给 `/api/alerts`（大屏每 30 s 打一次）**新增了一个「并发写就 500」的失败模式**。
  `src/db/credit.rs` 里的事务本来就写 `Immediate`，`sweep` 是仓库里唯一一个「先读后写」的事务。
- **`open_keys` 的那次 `SELECT` 留在事务内**（它是判定依据，必须在同一快照里读，否则并发下会误标解决）。
  这条与上一条合起来才成立：要同一快照 → 得在事务内读；不能在升级时炸 → 得 `Immediate`。
- 三条 SQL 文本、参数、`opened`/`resolved` 的自增位置**全部不动**。
- 失败时事务自动回滚 → 语义从「写了一半」变成「什么都没写」，这是**变好**：
  原来中途 `?` 早退会留下半个告警状态。
- 现有测试 `alert.rs:574 sweep_tracks_open_read_and_resolve` 覆盖
  新出现 / 还在 / 参数刷新 / 清空 / 回归 五个分支，改完必须原样通过。

### 3.4 D：迁移 V38，删两个前缀冗余索引

`CURRENT_SCHEMA_VERSION` 从 37 → 38（`src/db/schema.rs`）。新增块紧跟 `:349-357` 的 V37：

```rust
if current_version < 38 {
    // (api_key_id) 是 (api_key_id, created_at) 的前缀，(backend_id) 是 (backend_id, created_at) 的前缀：
    // EXPLAIN QUERY PLAN 实测优化器从未选中前者（见 docs/algorithm-optimization-plan.md §1.6），
    // 它们只按行付写放大。DROP IF EXISTS 可安全重跑（版本回卷测试会重放这一段）。
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_stats_api_key;
         DROP INDEX IF EXISTS idx_stats_backend;",
    )?;
    conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 38)", [])?;
    tracing::info!("Database migrated to V38 (drop prefix-redundant request_stats indexes)");
}
```

设计决定（三条都要写进注释，否则后人会「顺手加回来」）：

1. **不回改 `:615` / `:805` 的历史迁移字符串。** 已建库不会重放它们；改了只影响全新库，
   而全新库本来就要从 V1 一路跑到 V38，V38 的 `DROP` 会把它们清掉。
   代价是全新库多建再删一次（微秒级），换来的是**一处真相**：谁是冗余的，只在 V38 说一次。
2. **不 `VACUUM`。** 审计记录过：外部连接做 VACUUM + checkpoint 曾把 2,744 MB 压到 21.6 MB，
   但那是人工批准的独立操作，且要求实例停止。这里只删索引定义，空间留给既有的周期性压缩。
3. **`idx_stats_created` 不动**，理由见 §1.6 末尾。

新增一个迁移测试，形状照抄 `schema.rs:1277-1290` 的回卷测试：

- 把 `_schema_version` 改回 37 → `run_migrations` → 断言 `version == CURRENT_SCHEMA_VERSION`；
- 断言 `pragma_index_list('request_stats')` 里**没有** `idx_stats_api_key` / `idx_stats_backend`，
  **仍有** `idx_stats_api_key_created` / `idx_stats_backend_created` / `idx_stats_created` /
  `idx_request_stats_api_key`；
- 断言重跑幂等（连跑两次不报错）。

---

## 4. 实施顺序

严格按此顺序，每步之间可独立提交（`feedback-commit-before-deploy`：先检查点提交，再碰安装实例）。

1. `git status` 确认工作区干净 → 打一个检查点提交。
2. **B**（最小、纯前端、可独立验证）→ `npm --prefix frontend run build` → 人工开 `/wall` 下钻单模型、hover 五张图。
3. **A** → 同一次构建里验证；Network 数请求数；把后端进程临时 `kill` 掉再点一次自动刷新，确认 failbar 点名且**没有任何面板显示 `0`**。
4. **C** → `build.bat test`（必须走包装脚本，见 `agents.md` 的 D8050 陷阱）。
5. **D** → `build.bat test`，跑 §3.4 的新迁移测试 + 全部既有 schema 回卷测试。
6. `python -m pytest e2e -q` 全量。（实施时 HEAD 上那条 `test_provider_to_request` 已知失败已不复现；
   本轮全量的唯一红项是 `/settings` 的像素基线，见 §8 第 5 轮。）
7. 视觉回归：`python -m pytest e2e/test_visual.py`。**期望 0 像素差异**——
   这一轮全是取数与索引，不允许任何渲染变化。若 `/wall` 出现差异，说明 A 的合并改到了读数，停下来查。
   （实测结果见 §8 第 5 轮：`/traffic` 与 `/wall` **0 差异**；`/settings` 的 +76 px 与本轮无关。）

---

## 5. 验收标准

| # | 断言 | 手段 |
|---|---|---|
| A1 | 网络日志里不再出现 `/api/stats/proxy/{id}` / `/api/stats/platform/{id}`；批量端点各一次 | 新增 e2e `test_wall_batches_stats_and_names_the_batch_on_failure` |
| A2 | 时段切到 24h/7d/30d，KPI、平台占比、活跃网关读数与改前**逐字相同** | 同库同 seed 前后对比 |
| A3 | 批量接口失败时：failbar 点名「活跃代理」，读数条是 `—` 而不是 `0`/`0/0` | 同上（`page.route` abort 两个批量端点后重进 `/wall`） |
| B1 | 五张分面图的孤立点位置与改前完全一致 | `test_visual.py` 像素 diff（0 差异） |
| B2 | `test_model_timeline.py` 的 0 请求桶 / TTFT 缺口 / 未定价 lane 断言原样通过 | pytest |
| C1 | `sweep` 五个状态机分支行为不变 | `alert.rs:574` 原测试 |
| C2 | `sweep` 中途失败不留半写状态 | 新增单测 `sweep_is_atomic_when_a_write_fails`（触发器在第二条写入上 `RAISE(ABORT)`，断言库里 0 行） |
| D1 | 4 条代表性查询仍命中预期的**索引名** | 新增单测：断言 `EXPLAIN QUERY PLAN` 输出里出现允许的索引名（见下方说明） |
| D2 | V38 幂等 + 回卷可重放 | 新增迁移测试 |
| 全 | `cargo test` 全绿、e2e 除已知 1 条外全绿、视觉回归 0 差异 | §4 步骤 6-7 |

**A1 为什么不断言「一轮 = 9 个请求」**：初稿写的是请求数上界，落地时改成「禁止逐个实体查 + 批量端点各一次」。
实测一轮 `/wall` 是 **12 个**数据请求（`auth/status` + overview/proxies/platforms/errors/latency/model-summaries/
alerts/stats-proxies/stats-platforms/model-timeseries/requests），里面一半与本轮无关。钉死总数会让任何
**无关**的新接口都来改这条断言；而真正会退化的东西是「按实体个数线性增长」，所以断言的是
**网络日志里不许出现带 id 的统计路径**——这条与 P、Q 成正比，与页面新增多少块无关。
守卫有效性已实测：把 `Dashboard.tsx` 回退到改前重新构建，该用例立刻红并列出那两个 id 化 URL。

**D1 为什么不断言计划字符串**：初稿写的是逐字比对，落地时改成「输出里包含允许的索引名 + 白名单」。
理由是 SQLite 的 plan 文本不属于稳定契约——`SEARCH request_stats USING INDEX idx_stats_backend (backend_id=?)`
和 `... (backend_id=?) AND (created_at>?)` 是同一件事的两种写法，随 SQLite 版本变。
真正要守的是**语义**：这四条查询必须仍走复合索引，而不是退化成的单列索引或全表扫，
所以断言的是索引名（`idx_stats_api_key_created` 在、`idx_stats_api_key` 已不存在），换 SQLite 版本不会假红。

---

## 6. 明确不做（附理由）

| 项 | 为什么这轮不做 |
|---|---|
| **配额计数增量表**（`handler.rs:393` 注释里的「每请求一次全历史聚合」） | **注释已过期。** 实测：V12/V35 已建 `idx_stats_api_key_created`，`EXPLAIN QUERY PLAN` 是 `SEARCH ... USING INDEX`；200k 行库里单 Key 40k 行时，全历史聚合 17.0 ms、`quota_window=day` 2.8 ms。加上 2 s TTL 缓存（`handler.rs:583-611`），热路径上根本不到每请求一次。要改的是**注释**（本轮顺手改），不是数据模型。真要做增量表，得同时回答「删除虚拟模型/清理统计后分子怎么回退」——`db/api_key.rs:256` 明说分子是「现存统计里的用量」而非不可变台账，增量表和这个口径天然冲突。单独立项。 |
| `format.ts` 的 `toLocaleString()` 记忆化 | V8 内部有 locale 缓存，审计测过静态资源和 FCP 都不是瓶颈。属于「化妆」，按 `optimization-must-not-touch-behaviour` 要明说：不改也不会有人感觉到。 |
| `Settings.tsx:196` 滚动高亮加 rAF 节流 | 只读 `getBoundingClientRect`、不写样式，不构成强制同步布局循环。当前 sections 是个位数。 |
| **前端 SSE 解析「O(n²)」** | **误报，撤回。** `payloadView.ts:510-522` 是一次 `split` + 一次 `for` + Map 累加；`dataLines.join('\n')` 只作用于当前事件的几行。整体线性。 |
| **负载均衡 `least_connections` O(n)** | **不是缺陷。** LeastConnections 的定义就是扫全部候选取最小值（`lb/mod.rs:91-101`），且已做过复用 `String` 的分配优化。 |
| `reasoning_cache` 淘汰 / `latency_percentiles` | 已分别修成 O(N log N)（`reasoning_cache.rs:99-126`）和一次有序扫描（`stats.rs:369-394`），注释里记录了原 O(N²) / 4 遍扫描。无需再动。 |

---

## 7. 不要碰

- `agents.md` 里那条大屏约定：**取不到永远不许渲染成 `0` 或「无数据」**。A 的所有分支都必须服从。
- `—`（没定价 / 没样本）与 `0`（真的是零）的区分，`ModelTimeline.tsx:116-117` 的注释。
- `COALESCE(b.platform_id, 'unknown')` 的悬挂行兜底（`stats.rs:1978-1980`）：批量版换的是**取数方式**，不是归属口径。
- 历史迁移字符串（`:615`、`:805` 等）一律不改。
- `sweep` 三条 SQL 的文本与 `opened` / `resolved` 的计数时机。
- 视觉基线：本轮**不允许** `UPDATE_BASELINES=1`。有差异就是 bug。

---

## 8. 复核记录

| 轮次 | 发现 | 处理 |
|---|---|---|
| 1 | 子代理报「前端 SSE 解析 O(n²)」「`least_connections` O(n) 是缺陷」「`normalize_bound` 重复解析」 | 逐条读码后**撤回前两条**（§6）；`normalize_bound` 每次统计调用 2 次，不是循环内，不构成问题 |
| 1 | 子代理把 `Dashboard.tsx` 里 `useMemo` 已缓存的派生计算（`chartData` / `periodTotals` / `modelTotals`）报成「每次渲染重算」 | 核对依赖数组后撤回：都已正确 `useMemo` |
| 2 | 「大屏有健康看板面板」的假设 | 复核发现 `healthBoard` 只有读数条（`:1289`）没有 `PanelFailed` 分支 → §3.1 第 4 点显式处理，并把「若面板不存在则跳过」写清 |
| 2 | 「`platform_stats_all` 不返回 `platform_name`，大屏可能需要」 | 复核：大屏从 `listPlatforms()` 拿名字，按 id 合并 → 不需要，不新增 SELECT 列 |
| 3 | 「批量接口返回 `[]` 就等价于取到」 | 否。`[]` 是合法的空结果，必须用 `null` 承载失败，否则违反 §7 第一条 → 写进 §3.1 第 1 点 |
| 3 | 想顺手删 `idx_stats_created` 省空间 | **否决**：它不是任何索引的前缀，且实测查询计划选中了它，删了 `activity_heatmap` / `latency_percentiles` 会退化成全表扫 |
| 3 | 「配额全历史聚合是 O(n) 热点，值得做增量表」 | 实测 `EXPLAIN QUERY PLAN` + 计时后**降级为「改注释」**（§6 第一条），避免为一个已被索引解决的问题引入计数表与回退难题 |
| 4 | `sparseDot` 用 `index` 依赖「`buckets` 与图 `data` 同序」 | 已核：5 张图都是 `data={buckets}`（改后 `:385 :412 :429 :453 :471`）。风险用注释 + `test_model_timeline.py` 既有孤立点断言兜住 |
| 5 | **本文 §3.2 初稿的 `useCallback` 是我自己的设计错误** | `sparseDot(key,color)` 在 JSX 里被调用，工厂 memo 化不减少任何分配。实现按 §3.2 现稿（只删 `findIndex`）落地，文档同步改掉 |
| 5 | 落地时工作区**不干净**：8 个文件带着用户未提交的「流式空闲超时 + `ConnectionError` 重试」改动，且与 §3.1/§3.4 的目标文件重叠（`Dashboard.tsx` 不在其中，`handler.rs`/`schema.rs` 在） | 没有 `git stash`、没有替用户提交（`feedback-commit-before-deploy` 说的是**用户**先打检查点）。改为只动 §3 列出的行、每次编辑前 `git diff` 确认没踩到别人的 hunk（实测 `handler.rs` 的用户改动从 `:1057` 起，注释修改在 `:393`，不冲突）。已在对话里提示用户先自行提交 |
| 5 | `PlatformWithStats` 声明了 `platform_type` / `base_url`，批量接口的行里没有 | 全文件 grep 确认**零引用**后删除两字段。若保留，`npx tsc -b` 会在 A 的合并处报错 |
| 5 | 视觉回归 `/settings` 报「整页尺寸变了：基线 (1440, 2446)，本次 (1440, 2522)」 | **与本轮无关，已实测归因**：把**只**属于用户的 `Settings.tsx` 临时回退（已备份 `/tmp/Settings.user.tsx` 并原样还原），本轮四处改动全部保留并重新构建 → `/settings` 单用例**通过**（0 差异）。所以 +76 px 来自那两个新输入框（`stream_timeout_secs` / `stream_idle_timeout_secs`）。`/traffic` 与 `/wall` 本来就 **0 像素差异**，这才是 A/B 未改渲染的证据。按 §7 未执行 `UPDATE_BASELINES=1`——基线该由改动设计的人在自己那笔提交里重生 |
| 5 | `test_clients.py::test_codex_extras_render_from_the_server_field_table` axe 报 `color-contrast` 1.19:1（fg `#04211f` / bg `#1c2e2e`） | **先证明不是我改的**（存下我改的 2 个前端文件 → `git checkout --` → 重新构建 → 仍然红 → 还原再构建），再定位：临时诊断用例打印真实状态，测得 apply 后按钮**先是禁用**（bg `#171b1b`、文字 `#b8beba`，`:not([data-variant]):disabled` 覆盖其实生效），解除禁用瞬间文字跳成 `#04211f` 而背景还要渐变 150 ms，axe 撞在中间帧上。**等过渡结束后同一处 axe 零违规** → 是 `agents.md` 记过的审计时机坑，不是配色缺陷。修法：把兄弟用例里已有的「`!b.disabled` + 500 ms 沉降」守卫提成 `_audit_settled()`，本文件两处审计统一走它（别的文件每页只审一次，不存在此坑） |
| 5 | §6 第一条说的「注释过期」——`handler.rs:393` 把配额分子写成「每请求一次全历史聚合」 | 已按 `db/api_key.rs:262` 的真实 SQL（`WHERE api_key_id=?1 AND created_at>=?2`，走 `idx_stats_api_key_created`）改写注释，并写上实测值（200k 行库、单 Key 40k 行：全历史 17 ms / 窗口 day 2.8 ms）。「增量表」仍按 §6 单独立项 |
| 6 | **§3.3 初稿的 `conn.transaction()` 是我引入的真实回归** | 用户追问「有没有引入新 bug」后自查发现：DEFERRED 事务在**读→写升级**时不查 busy handler，实测 16 µs 返回 `SQLITE_BUSY_SNAPSHOT(517)`（`busy_timeout=5000` 无效），而同场景 IMMEDIATE 等 259 ms 后成功。改 → `TransactionBehavior::Immediate`，并补确定性单测 `sweep_waits_for_the_write_lock_instead_of_failing`（通道同步的持锁者 + 定时释放，断 `is_ok()` 且 `took > 100ms`）。变异验证：改回 `conn.transaction()` → 该用例以 `DatabaseBusy` 红 |
| 6 | §1.5 与代码注释里的「每轮付 `2k+1` 次 fsync」是**未证实的量级** | 实测只有 5.6×（1.5 → 0.27 ms/轮），且 WAL + `synchronous=NORMAL` 下逐语句提交根本不 fsync。两处注释与本文 §0/§1.5/§2 已改成实测口径 |
| 6 | A 的批量/单条口径只靠 e2e 断言，没有后端对账 | 补 `db/stats.rs::bulk_stats_match_the_per_entity_queries_field_by_field`：3 平台 / 3 虚拟模型 / 3 后端 + 7 行种子（含一个无统计的 `px-ghost`、一个后端已删的悬挂行），逐字段对齐 `*_stats_all` 与 `*_stats`。变异验证：把 `COUNT(rs.id)` 改成 `COUNT(*)` → 红（LEFT JOIN 空匹配会被数成 1 笔请求） |
| 6 | B 的 `sparseDot` 改动**没有任何测试能变红**：`/wall` 像素基线里时间线面板默认折叠，覆盖不到 | 补 `e2e/test_model_timeline.py::test_isolated_bucket_gets_a_dot`，按 lane 钉死「画了几个点」：流量与失败 2 / 延迟与首字 3 / 输出速度 2 / 成本 1 / Token 构成 0（柱状不该补点）。变异验证：`index - 2` 邻居错位 → 红；**「永远画点」→ 不红**，已记为覆盖局限（本种子下每个非空桶都是孤立桶，两种行为等价），未为凑红而改种子 |
| 6 | `test_clients.py` 的 axe 时机守卫只在一处用例里 | 提成模块级 `_audit_settled()`，本文件两处审计统一走它（见上一轮） |

---

## 9. 验收结果（2026-11-20）

| 项 | 手段 | 结果 |
|---|---|---|
| A1 | 新 e2e `test_wall_batches_stats_and_names_the_batch_on_failure` | **绿**。网络日志 13 个数据请求（含 `auth/status`），带 id 的统计请求 **0 个**；把 `Dashboard.tsx` 回退到改前重新构建后**同一用例立刻红**，并列出那两个 id 化 URL → 守卫不是摆设 |
| A2 | `e2e/test_wall.py` 全量 + `test_model_timeline.py` | **绿**。读数条 8 张卡、矩阵、热力、趋势断言原样通过；`/wall` 像素 **0 差异** |
| A3 | 同上（`page.route` abort 两个批量端点后重进 `/wall`） | **绿**。failbar 点名「活跃代理」，该读数条是 `—` 而不是 `0 / 0` |
| A4 | 新单测 `bulk_stats_match_the_per_entity_queries_field_by_field`（批量 vs 逐实体逐字段对账） | **绿**。变异：`COUNT(rs.id)` → `COUNT(*)` 红（LEFT JOIN 空匹配会被数成 1 笔） |
| B1 | `e2e/test_visual.py` `/wall` | **0 像素差异** |
| B2 | `e2e/test_model_timeline.py` | **绿**（0 请求桶、TTFT 缺口、未定价 lane、矩阵下钻、axe、1280/1440/1920 布局探针） |
| B3 | 新 e2e `test_isolated_bucket_gets_a_dot` | **绿**。变异：邻居取 `index - 2` → 红；「永远画点」→ **不红**（本种子下每个非空桶都孤立，两种行为等价），已记为覆盖局限 |
| C1 | `src/db/alert.rs::sweep_tracks_open_read_and_resolve` | **绿**，SQL 文本与计数时机未改（`git diff -U0` 里 4 条语句只以「同文本换缩进」的形式成对出现） |
| C2 | 新单测 `sweep_is_atomic_when_a_write_fails` | **绿**，`cargo test --lib` 350 → **351 passed / 0 failed** |
| C3 | 新单测 `sweep_waits_for_the_write_lock_instead_of_failing`（§8 第 6 轮的回归钉子） | **绿**。变异：改回 `conn.transaction()` → `DatabaseBusy` 红 |
| D1 | 新单测 `v38_drops_only_prefix_redundant_indexes` | **绿**。2 个冗余索引不在、6 个仍被依赖的索引在、4 条查询的计划分别落在期望索引名上 |
| D2 | 同上（回卷到 37 后连跑两次 `run_migrations`） | **绿**，幂等 |
| 全 | `cmd /c build.bat test --lib` | 最终构建 **353 passed / 0 failed** |
| 全 | `npx tsc -b` | 无错误 |
| 全 | `python -m pytest e2e -q` 全量（最终构建，前后端均已重建） | **51 passed / 1 failed**，唯一红的是 `/settings` 整页高度基线（测量归因见 §8 第 5 轮：用户新增的两个超时输入框 +76 px，本轮改动回退后单用例 0 差异）。此前记录的 `test_provider_to_request` 已知失败本轮**不再复现** |
| 全 | 最终构建复验 | `test_wall.py` + `test_model_timeline.py` + `/traffic` + `/wall` 视觉 = **14 passed**（含 A1/A3 新守卫），排除"只在中间产物上绿" |
