# 单模型分时性能 · 设计与落地计划

> 目标：概览页选中一个模型，按与「模型请求统计」完全相同的时段和粒度，看该模型每个时间桶的请求、失败、输入、新增输入、输出、缓存、成本、耗时、首字与速度。
>
> **结论先行**：不要把这 10 个指标画在同一根 y 轴上。推荐「共享 x 轴 + 共享悬停读数条的 5 条分面小图」，数据直接复用现有 `/stats/model-timeseries` 响应做客户端派生，保证时间桶与现有统计逐桶一致；后端只补 5 个聚合字段，不新增接口、不新增迁移。

---

## 0. 建议摘要

| 决策 | 建议 | 理由 |
|---|---|---|
| 视图形态 | 分面小图，不是单图 10 线 | 10 个指标量纲差 4–6 个数量级，一根线性轴必然把多数线压平 |
| 入口 | 现有「模型请求统计」面板加「全部模型 / 单模型」模式 | 时间范围、粒度、数据源天然共用，不新增页面 |
| 数据源 | 复用同一个 `tsData`，单模型在客户端派生 | 结构性保证桶边界一致；切模型零请求、瞬时 |
| 后端 | 补 `fresh_input_tokens`、`cache_write_tokens`、`total_cost`、`priced_requests`、`ttft_samples` | 其余 8 个指标已在现有响应里 |
| 空值 | 计数缺失 = 0，均值缺失 = null | 均值画成 0 会看起来像性能突然变好 |
| 平滑 | `monotone`，桶少时自动 `linear`；禁止 `basis` / `natural`；不做隐式移动平均 | 曲线只是视觉插值，桶内真值始终由 tooltip 和表格给出 |
| 最后一桶 | 标记「进行中」 | 所有页头时段以 now 为终点，最后一桶天然不完整 |
| 成本 | 单独一条 lane；未定价时用文字说明，不画空图 | 成本与 token 在单价不变时同形，但覆盖率和定价变更需要被看见 |
| 一致性 | 摘要条直接读 `/stats/model-summaries` 同一行 | 矩阵与摘要条逐项相等，避免前端重算加权平均 |

---

## 1. 需求与边界

### 1.1 用户故事

作为一个看网关的人，我想选中一个模型，看它在每个小时/天的表现，以回答：

- 这个时段变慢，是流量涨了、失败多了，还是模型本身变慢？
- 首字变慢还是生成变慢？（TTFT vs output speed）
- 缓存命中是不是掉了？新增输入是不是涨了？
- 成本涨是因为用量涨了，还是因为失败/未定价导致覆盖不全？
- 失败高峰落在哪个时段，和流量高峰是不是同一桶？

### 1.2 必须满足

- 时间跨度、粒度、桶边界与现有「模型请求统计」完全一致；同一份数据，不是两套 SQL。
- 桶内所有指标同屏可见，可在任意 lane hover 读精确值。
- 空桶、缺失桶、未定价、无流式请求都不能渲染成 0。
- 不改变现有全部模型视图与模型性能矩阵的口径。
- 不新增页面/路由；遵守控制台 i18n、明暗主题、11px 字号下限、axe 0 违规。

### 1.3 不做

- 不做多模型叠加对比（已有全部模型视图）。
- 不做逐请求实时流（已有请求明细）。
- 不做平台维度拆分、p95/p99 分位数、告警、CSV 导出（列入后续）。
- 不新增数据库迁移或索引；先用现有聚合，测量后再决定。

---

## 2. 交互：同一面板的第二种视图

### 2.1 位置

现有趋势面板在 `frontend/src/pages/Dashboard.tsx:1089`，已经占整行。在标题右侧增加模式切换，不新开面板、抽屉或路由：

- 时间/粒度控制天然共用，不会出现两个面板各自 `rangeToFromTo` 的漂移。
- 用户的心智是「同一张统计图，换个模型看」，不是另一个页面。
- 模型性能矩阵的模型名可以成为入口，不需要新导航。

### 2.2 线框

```text
┌ 模型请求统计 ────────────────────────────────────────────────────────────────┐
│ [全部模型 | 单模型] [模型 gpt-4o ▾]                [按小时] [刷新]            │
│ 请求 12,430 · 失败率 1.8% · 耗时 1.2s · 首字 320ms · 速度 42 · 成本 3.21(86%) │
│                                                                              │
│ 流量与失败   ┤       ╱╲        ╱╲                                            │
│              ┤  ____╱  ╲______╱  ╲___                                        │
│ 延迟 / 首字  ┤      ╱‾‾╲___╱‾‾╲                                              │
│ 速度         ┤  ‾‾‾╱     ╲__╱‾‾‾                                              │
│ Token        ┤  ▓▓▓▒▒░░  + 输出线                                             │
│ 成本         ┤      ╱‾‾╲___                                                   │
│                                                                              │
│ 最后一桶进行中 · 小时桶按本地时区，日/月/年桶按 UTC · 与模型请求统计同一响应   │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 模式切换

- 默认「全部模型」= 现状完全不变。
- 「单模型」显示模型 Select，隐藏指标 SegmentedControl 与堆叠开关（所有指标都画，不再选一个）。
- 粒度按钮保留，作用于两种模式；时段仍由页头控制，面板内不出现第二套时间控制。
- 模式切换只在 `tsData` 至少有一个模型时出现；空库/全新安装不出现死控件，也让 `/wall` 零流量的像素基线保持不变（见 §7.3）。
- 模式状态放 Dashboard 组件 state；不写 URL（hash 路由下 query 不是现有模式）。可选持久化到 `sessionStorage`，默认不持久化。

### 2.4 模型选择

- 选项 = 当前时段的 `tsData.data` 里出现过的 `model_id`，按请求数降序，同数按 id 升序（确定性，截图与测试需要）。
- 默认 = 第一个（请求最多的模型）。
- 模型性能矩阵的模型名从 `<td>` 改成按钮，点击 = 切到单模型 + 选中该模型 + 面板滚入视口（尊重 `prefers-reduced-motion`）。
- 选中模型在切时段后若不在新窗口：保留选中值并把它补进选项，面板显示「该模型在本时段没有请求」，不静默改成别的模型。自动跳模型会让用户以为看的是 A，实际是 B。
- `unknown`（backend 已删除）是合法选项，标签本地化为「未知（后端已删除）」。
- 明确：这里的「模型」是后端 `model_id`，与矩阵/趋势同口径，不是网关虚拟模型名。写在 Select 的 title 或面板副标题里。
- 不做整行点击：行里已有失败数按钮，整行可点会让键盘与读屏语义变糊。

### 2.5 刷新与失败

- 沿用 `loadTs` 的 30s 静默刷新；模式切换不发请求（数据已在 `tsData` 里）。
- 失败：`modelStats` 失败块同时覆盖两种模式，面板显示 `PanelFailed`，failbar 点名「模型请求统计」；不得显示 0 或旧值冒充新值。
- 完全没有数据且未失败：保持现有空态，不显示模式切换。

---

## 3. 图表：为什么不是 10 条线一根轴

### 3.1 量纲问题

| 指标 | 典型量级 | 单位 |
|---|---|---|
| 请求 / 失败 | 1–10⁴ | 次 |
| 输入 / 新增输入 / 输出 / 缓存 | 10²–10⁷ | tokens |
| 耗时 / 首字 | 10²–10⁵ | ms |
| 速度 | 1–200 | tok/s |
| 成本 | 10⁻⁴–10² | 货币（用户定价） |

- 同一根线性轴：token 线会把请求、成本压成贴着 0 的直线；按成本缩放，延迟又顶出屏幕。
- 双轴最多容纳 2 个指标；10 个指标需要 5 根右轴，交叉点没有意义。
- 归一化成指数能塞进一张图，但读数从「1,240 次 / 1.2s」变成「37 / 82」，不适合排障。

**一句话：同时看 ≠ 同时画。**「同时」应该由共享悬停读数条和一张合并数据表提供，而不是 10 条线互相压扁。

### 3.2 推荐：共享 x 轴的分面

| # | Lane | 序列 | y 轴 | 说明 |
|---|---|---|---|---|
| 1 | 流量与失败 | 请求主线；失败率虚线 | 左：次；右：% | 只有这一条允许副轴；失败数在流量小的桶里会被请求线淹没，图上画可跨桶比较的失败率，精确失败数放 tooltip |
| 2 | 延迟与首字 | 平均耗时；平均首字 | ms | 首字虚线；无流式样本时断线 |
| 3 | 速度 | 输出速度 | tok/s | 含失败耗时，见 §4.1 |
| 4 | Token | 输入构成（新增 / 缓存读 / 缓存写堆叠）+ 输出线 | tokens | 堆叠顶 = 输入总量；输出线可与输入差异很大，见 §3.6 |
| 5 | 成本 | 估算成本 | 货币 | 未定价断线 + 文案；单价不变时形状与 token 同形 |

- 顺序按排障问题排：先看流量/失败，再看延迟/首字/速度，最后看 token/成本。用户原列表的顺序不是阅读顺序。
- 缓存命中率放 tooltip 与摘要脚注，不单独画线：它是比率，和 token 量不是同一量纲；单独画会再引入一根右轴。
- 每条 lane 高度 84–96px（全屏 1.2×），共享 x 轴；只有最下方的可见 lane 显示 x 刻度，上方 lane 保留轴线不留刻度。
- 成本 lane 只在 `priced_requests > 0` 时出现；否则在摘要条里写「未定价，成本不可估算」，不画空图。
- 窗口内某项为 0 的可选序列按下述规则收起，并在 lane header 用文字说明「本期 0 失败 / 本期无缓存 / 本期无流式请求」，不能让人把「没画」当成「没测」：
  - 失败率：`failed_requests === 0` 时只画请求线。
  - 缓存读/写：两者都为 0 时只画新增输入与输出。
  - 首字：`stream_requests === 0` 时不画首字线。
  - 输出：`token_output === 0` 时不画输出线。
- 若摘要行取数失败（`modelMatrix` 块），无法判断哪些序列为 0：按「全显示」处理，不根据未知信息做隐藏。

### 3.3 「同时」由什么提供

- **一条共享悬停读数条**：hover 任意 lane，读数条从「本时段汇总」切换成该桶的 10 个指标：请求、失败数与失败率、输入总量、新增、缓存读/写、输出、耗时、首字、速度、成本，外加缓存命中率与成本覆盖。实现上没有用 5 个浮动 tooltip：它们会同时弹出、互相遮挡，在大屏上读不清；一条固定读数条 + 一条跨 lane 的同步竖线更稳定。
- **一张合并的 sr-only 数据表**（period × 所有指标），供读屏与复制。
- 不是 10 条线同时压在一根轴上。这样既满足「同时看」，又不让任何一条线被压扁。

### 3.4 如果坚持单图（备选，不推荐默认）

- 「指数视图」：每个指标按窗口内最大值归一为 0–100，tooltip 显示原值；图例注明「100 = 本窗口峰值」。
- 只适合看形状/相位（谁先动），不能读量级；交叉点没有含义。
- 放入模式切换的第三项或作为后续实验，不作为默认视图。

### 3.5 平滑规则

- `type="monotone"`：曲线只是视觉插值，经过每个真实点；tooltip 与 sr-only 表永远给桶内真值。
- 桶数 < 8 或只有单点时自动 `linear`，避免把两三个点画成波浪。
- 禁止 `basis`（不经过数据点，读数会骗人）与 `natural`（可能过冲到负值）。
- `connectNulls={false}`：缺失桶必须断线，不能跨过去连。
- 不做隐式移动平均。若以后要「趋势平滑」，单独开关并标注窗口（如「3 桶移动平均」），默认关。
- 不把「最后一桶未完成」当作缺失去断线；单独标记（见 §4.4）。

### 3.6 颜色与版式

- 全部模型视图：一个模型一个颜色（现有 `MODEL_PALETTE`），保持不变。
- 单模型视图：颜色表示指标族，不表示模型；模型身份由标题里的色点 + 名称承担。
  - 主指标（请求、延迟、速度、成本）：`--accent` 单色。
  - 失败：`--err`（语义色，唯一例外）。
  - 失败率：`--err` 虚线 + 右轴，与失败数区分。
  - 输入构成：accent 三档明度（新增 100% / 缓存读 55% / 缓存写 30%）。
  - 输出：现有调色板的青色档（暗 `#14B8A6` / 亮 `#0B7285`），与 accent 区分。
- 不用彩虹、不用发光；堆叠面积填充 22 / 12 / 6%；线宽 1.5；计数线 `dot={false}`。
- 均值/成本/失败率在稀疏桶里会被 null 断线：如果 `dot={false}`，孤立点连点都没有，图上看起来像没数据。实现上只在「前后都没有非空邻居」的孤立点补一个 2.5px 点，既不跨缺口连线，也不给稠密序列添噪点。
- 桶数 < 8 时曲线自动切 `linear`，避免两三个点被画成波浪；其余用 `monotone`。图表关闭入场动画（`isAnimationActive={false}`），保证 30s 刷新和像素截图不会停在半张图。
- 字号遵守 11px 下限：轴/图例 11，正文 12；所有数字 `tabular-nums`。
- 图例放 lane header 右侧，静态展示（v1 不做点击隐藏，避免 a11y 与状态复杂度）。
- 输出线通常远小于输入（例如 2K vs 100K），会贴着底部；这是真实比例，精确值放 tooltip 与摘要条，不做第二根 token 轴。

### 3.7 可访问性

- 每条 lane 的 `ResponsiveContainer` 外层 `role="group"` + `aria-label`（模型 + 指标 + 窗口 + 桶数）。
- 整个面板一张 `ChartAltTable`（而不是每条 lane 一张）：head = 周期 + 指标列，rows = 每个桶；caption 包含模型、时段、粒度、时区说明。全部模型视图继续用原来的表，两者互斥渲染。
- 模型 Select 用 `Field` 或 `aria-label`；模式切换用有名字的 SegmentedControl；矩阵模型名按钮有 `aria-label`。
- 不只靠颜色区分：失败率、首字用虚线，图例有文字；axe 跑 `/wall` 两种配色 0 违规。
- 键盘：模式切换、Select、模型名按钮都必须是有效 Tab stop 且有焦点环；`e2e/test_keyboard.py` 的最低 stop 数需要更新。

---

## 4. 数据口径（最容易出错的部分）

### 4.1 指标定义

| 指标 | 前端字段 | 口径 | 注意 |
|---|---|---|---|
| 请求 | `requests` | `COUNT(*)` | 含失败、含重放 |
| 失败 | `failed_requests` | 非 2xx | 含 429 / 超时 / 连接失败 |
| 失败率 | 派生 `failed / requests` | 0 请求时为 null | 不是 0% |
| 输入 | `token_input` | 总提示词 token，含缓存读/写 | 不是 fresh |
| 新增输入 | `fresh_input_tokens` | `input − cache_read − cache_write`，max 0 | 与矩阵同公式 |
| 缓存读 | `cache_read_tokens` | `SUM` | 缓存命中量 |
| 缓存写 | `cache_write_tokens` | `SUM` | 目前仅 Anthropic 等 |
| 输出 | `token_output` | `SUM` | — |
| 成本 | `total_cost` | `SUM(cost)`，仅已定价行 | 全未定价为 null；0 是合法值 |
| 耗时 | `avg_latency_ms` | 桶内 `AVG(latency_ms)` | 含失败请求的等待时间 |
| 首字 | `avg_ttft_ms` | 桶内非 NULL 的 `AVG(ttft_ms)` | 无样本时前端置 null |
| 速度 | `output_speed_tps` | `SUM(output)*1000 / (SUM(latency) − Σ COALESCE(ttft,0))` | 含失败耗时，是「有效吞吐」不是纯模型速度 |
| 成本覆盖 | `priced_requests` | `COUNT(cost)` | 与 `requests` 的比值 |
| 首字样本 | `ttft_samples` | `COUNT(ttft_ms)` | 新增字段，用来区分「0ms」与「没有样本」 |

- **成本公式**：输入总 token（含缓存）× 输入价 + 输出 × 输出价。缓存没有独立单价，所以缓存命中不会让成本线下降，只会让新增输入线下降。这条要写进成本 lane 的 title 或 tooltip。
- **速度口径**沿用现有矩阵/KPI，避免同一页两个「速度」。代价是失败请求的耗时进入分母；如果以后要「纯成功请求速度」，作为新指标另开一栏，不偷偷改旧口径。
- **耗时**同样含失败请求（例如 120s 超时会拉高均值），它回答的是「用户实际等了多久」，不是「成功请求的纯模型耗时」。

### 4.2 空桶 vs 0 vs 缺失

| 情况 | 计数类（请求/失败/输入/输出/缓存） | 均值类（耗时/首字/速度） | 成本 |
|---|---|---|---|
| 桶内 0 请求 | 0（真值） | null（断线） | null（无已定价请求） |
| 桶内请求但无流式 | 正常 | 首字 null；速度按现有公式仍可能有值 | 正常或 null |
| 桶内请求但全部失败 | 失败 = 请求数 | 耗时仍画（用户等待）；速度可能为 0 | 通常 null（失败通常无 usage） |
| 未定价 | token 正常 | 正常 | null，摘要条显示 — |
| 历史行早于 V7/V8/V15 | 缺字段按缺失处理 | 首字/缓存/成本断线 | 不补 0 |

- 计数 0 与「没取到」是两件事；均值 0 与「没有样本」也是两件事。后者一旦画成 0，看起来像性能突然变好。
- 失败率 0 请求 = null；tooltip 显示「—」。
- 成本 0 与未定价：0 是合法（免费模型/定价为 0），null 是没定价。`formatCost` 已按此区分，继续复用。

### 4.3 不要把桶值再平均一次

| 指标 | 窗口汇总怎么算 | 不能怎么做 |
|---|---|---|
| 请求 / 失败 / token / 成本 | 直接相加 | — |
| 耗时 | 按请求数加权：`Σ(avg_i × requests_i) / Σ requests` | 对桶均值再取平均 |
| 首字 | 按 `ttft_samples` 加权 | 按请求数加权（会把无样本桶算成 0） |
| 速度 | 用窗口的分子/分母重算，或直接读 `model-summaries` | 对桶速度取平均 |
| 缓存命中率 | `cache_read / (cache_read + fresh)` 用总量重算 | 对桶命中率取平均 |

- 摘要条直接读 `/stats/model-summaries` 的同一行，不从前端桶数据再算一遍：矩阵和摘要条的数字必须逐项相等。
- 验收不变量（见 §7.2）：`Σ buckets.requests = summary.requests`、`Σ cost = summary.total_cost`、按请求数加权的耗时 = `summary.avg_latency_ms`。速度只断言趋势，不做加权等式，因为它不是可加的。

### 4.4 最后一桶

- 页头所有时段都以 now 为终点，`fill_periods` 会包含 now 所在的桶（`to` 所在小时/天），所以固定时段的最后一桶必然是进行中的；`all` 模式只有最后一个有流量的桶是当前桶时才标记。
- 处理：tooltip 在最后一桶加「进行中」；面板底部一行「最后一桶尚未结束，读数会继续增长」；峰值/注解不把最后一桶算进去（v1 没有峰值注解，先保证文案）。
- 不把最后一桶从图里删掉：那会让图与矩阵窗口对不上，也看不到最新数据。
- 视觉上不要用虚线段（容易和「缺失/估算」混淆）；用 tooltip 后缀 + 脚注即可。
- 最后一桶请求数天然偏低、耗时样本天然偏少，读趋势时不要把它的「下降」当成真实回落。

### 4.5 时间桶与时区

- 现有实现：hour 粒度用 `datetime(created_at,'localtime')`（服务器本地时区）；day/month/year 用原始 UTC 字符串（`src/db/stats.rs:143`）。这是一个已存在的不一致，不是本次引入的。
- 单模型视图放大后会更显眼：UTC+8 的用户看「按天」，桶边界是本地 08:00，不是 00:00。
- 本方案不改口径（改会让历史对比和现有面板全部漂移）；在面板副标题/脚注写明：「小时桶按本地时区；日/月/年桶按 UTC」。如果之后要统一，单独做迁移与回归，不在这个功能里夹带。

### 4.6 历史数据覆盖

- 早于 V7 无 ttft/stream，早于 V8 无缓存，早于 V15 无成本；这些桶的相应线是断的，不是「当时为 0」。
- 固定时段的开始时间若早于库里第一条记录，补齐的桶是 0，会看起来像「长期无流量」。v1 在脚注写「0 = 该时段没有请求记录」；如果以后要更诚实，可以读 `oldest_request_at` 在 x 轴前画一条「无历史数据」的遮罩（后续）。
- 清理/保留期删掉旧行后，`all` 的桶集合也会缩短；这是预期行为，脚注说明数据保留策略。

### 4.7 已知限制

- 模型 ID 是后端 `model_id`；同一模型跑在多个平台会合并，平台差异不可见。
- `unknown` = backend 已删除的悬挂统计。
- 每次最终结果一行；重试的中间尝试不计入请求数（轨迹在 `request_attempts`）。重放请求计入（与矩阵一致），会在图上形成额外尖峰。
- 速度/耗时口径含失败；p95 未提供。
- 成本无缓存折扣；缓存收益只在 token 构成里体现。

---

## 5. 后端改造

### 5.1 字段

`src/models/stats.rs:172` 的 `ModelTimeSeriesPoint` 追加：

```rust
pub fresh_input_tokens: i64,
pub cache_write_tokens: i64,
pub total_cost: Option<f64>,
pub priced_requests: i64,
pub ttft_samples: i64,
```

- `ttft_samples` 让前端区分「没有流式样本」和「首字真的是 0ms」（现有 `COALESCE(AVG(ttft),0)` 无法区分）。
- 其余字段与 `model_summaries` 同口径，避免两套公式。

### 5.2 SQL

`src/db/stats.rs:168` 的 SELECT 追加：

```sql
COALESCE(SUM(rs.cache_write_tokens), 0) AS cache_write_tokens,
SUM(rs.cost)                            AS total_cost,
COUNT(rs.cost)                          AS priced_requests,
COUNT(rs.ttft_ms)                       AS ttft_samples
```

Rust 映射处：

```rust
let fresh_input = (token_input - cache_read_tokens - cache_write_tokens).max(0);
```

- 不新增接口、不改路由、不加查询参数。`/stats/model-timeseries` 仍是唯一数据源。
- 不新增迁移：字段都是聚合值，没有存储变化。
- 索引不动。现有 `idx_stats_created` 支撑范围过滤；加聚合字段不改变查询计划。除非实测变慢，否则不要加复合索引（先测量再归因）。

### 5.3 为什么单模型视图在客户端派生

- `tsData` 已经包含每个模型每个桶的数据；再发一次带 `model=` 的请求会得到几乎相同的行，却多一次扫描和一次「桶骨架可能不同」的风险。
- 客户端派生天然保证：同一份 `periods`、同一份桶、同一份 `to`，不存在两次请求跨秒导致的边界漂移。
- 切模型零请求、瞬时；30s 自动刷新只拉一次。
- 代价：payload 多 5 个数/点。按 7 天 × 小时 × 20 个模型 ≈ 3,360 点估算，增量可忽略；`all` 极端情况（4000 期 × 大量模型）需要实测，若超过预算再加 `model` 参数。届时必须单独提供全局桶骨架，不能只按模型过滤，否则稀疏模型的 x 轴会被压缩、平滑线会把相隔几天的点直接连起来。
- 这条是 §2「时间跨度一致」的结构性保证，不是靠两边各写一遍 `rangeToFromTo`。

### 5.4 测试

Rust 单测（`src/db/stats.rs` 的 `tests` 模块）：

1. 同一时段两个模型：确认点只含各自 `model_id`；新字段正确。
2. `fresh = input − read − write`，异常负数 saturating 到 0。
3. 全未定价桶：`total_cost = None`、`priced_requests = 0`；部分定价桶：`SUM` 只算已定价行。
4. 非流式桶：`ttft_samples = 0`、`avg_ttft_ms = 0`（前端据此置 null）；流式桶：samples > 0。
5. 桶汇总与 `model_summaries` 对齐：Σ requests / cost / priced 与同一范围 summary 相等；耗时按请求数加权相等。
6. 范围补齐：有空洞的范围仍然返回连续 periods；`all` 模式的 periods 来自全局数据，不因单模型稀疏而缩短。

---

## 6. 前端改造

### 6.1 文件

| 文件 | 改动 |
|---|---|
| `frontend/src/pages/Dashboard.tsx` | 模式状态、模型选择、矩阵入口、把 `tsData` 传给新组件 |
| 新组件 `frontend/src/components/ModelTimeline.tsx` | 悬停读数条 + 5 条 lane + 共享竖线 + sr-only 表；Dashboard 已 1557 行，不再往里堆 |
| `frontend/src/main.css` | `.wall-timeline-*` 样式 |
| `frontend/src/i18n.ts` | 新 key（zh/en 同步） |
| `frontend/src/api.ts` / `src/api/stats.rs` | 不改：继续用 `/stats/model-timeseries` |

### 6.2 派生函数

```ts
type Bucket = {
  period: string
  requests: number
  failed: number
  failRate: number | null
  tokenIn: number
  freshIn: number
  cacheRead: number
  cacheWrite: number
  tokenOut: number
  latency: number | null
  ttft: number | null
  speed: number | null
  cost: number | null
  priced: number
}

function buildBuckets(stats: ModelTimeSeriesStats, modelId: string): Bucket[] {
  const byPeriod = new Map(
    stats.data.filter(p => p.model_id === modelId).map(p => [p.period, p])
  )
  return stats.periods.map(period => {
    const p = byPeriod.get(period)
    if (!p) {
      return { period, requests: 0, failed: 0, failRate: null,
        tokenIn: 0, freshIn: 0, cacheRead: 0, cacheWrite: 0, tokenOut: 0,
        latency: null, ttft: null, speed: null, cost: null, priced: 0 }
    }
    return {
      period,
      requests: p.requests,
      failed: p.failed_requests,
      failRate: p.requests > 0 ? (p.failed_requests / p.requests) * 100 : null,
      tokenIn: p.token_input,
      freshIn: p.fresh_input_tokens,
      cacheRead: p.cache_read_tokens,
      cacheWrite: p.cache_write_tokens,
      tokenOut: p.token_output,
      latency: p.avg_latency_ms,
      ttft: p.ttft_samples > 0 ? p.avg_ttft_ms : null,
      speed: p.output_speed_tps,
      cost: p.priced_requests > 0 ? p.total_cost : null,
      priced: p.priced_requests,
    }
  })
}
```

- 计数缺失 = 0；均值缺失 = null；成本缺失 = null；绝不 `?? 0` 一刀切。
- 不修改现有 `chartData` 的透视逻辑（全部模型视图保持现状），新逻辑独立。

### 6.3 Recharts 配置要点

```tsx
<LineChart
  data={buckets}
  margin={{ top: 4, right: 38, left: 0, bottom: 0 }}
  onMouseMove={(state) => setHoveredPeriod(String(state.activeLabel))}
>
  <CartesianGrid strokeDasharray="3 3" stroke={cc.grid} vertical={false} />
  <XAxis dataKey="period" tick={showXAxis ? axisTick : false} tickLine={false} axisLine={{ stroke: cc.grid }} height={showXAxis ? 26 : 6} />
  <YAxis width={56} tick={axisTick} tickFormatter={...} tickLine={false} axisLine={false} allowDecimals={false} />
  {hoveredPeriod && <ReferenceLine x={hoveredPeriod} stroke={accent} strokeOpacity={0.55} strokeDasharray="4 4" />}
  <Line type={curveType} dataKey="requests" stroke={accent} strokeWidth={1.5} dot={false} connectNulls={false} isAnimationActive={false} />
  {/* 失败率那一层才加：<YAxis yAxisId="rate" orientation="right" domain={[0, 100]} /> */}
</LineChart>
```

- 上方 lane 隐藏 x 刻度（`tick={false}`），但保留 `XAxis` 以共享 domain；只有最下方的可见 lane 显示刻度。
- 失败率需要第二个 `<YAxis yAxisId="rate" orientation="right" domain={[0, 100]} />`，只在这一条 lane 用；其余 lane 不设第二轴。
- **左右轴宽必须固定**：所有 lane 的左 Y 轴宽 56px；有右轴的 lane 右轴宽 38px，没有右轴的 lane 用 38px 的右 margin 预留同样空间。否则同一条悬停竖线在不同图上会落到不同像素，读起来像两个时段。
- 左轴不要写 `yAxisId`（保持默认 0）：这样 `Line` / `ReferenceLine` 不写 id 时也有轴可挂；只有右轴用显式 `yAxisId="rate"`。这一条踩过坑：两个轴都用显式 id 时，悬停渲染 `ReferenceLine` 会触发 Recharts invariant，整棵 React 树被卸载。
- 悬停读数用 `state.activeLabel`（桶的 period）反查 `buckets`，读数条显示该桶全部指标；竖线用 `ReferenceLine` 在每条 lane 上画，不需要 Recharts 的浮动 Tooltip / syncId。
- Recharts 2.x + React 19：轴/元素必须是数组或直接子元素，不能包在 Fragment（`Dashboard.tsx:868` 的注释已记录），新组件沿用数组写法。
- 时间线关闭入场动画；现有大屏图表的 500ms 动画不变。

### 6.4 悬停读数条

- 默认显示「本时段汇总」的 10 个 chip：请求、失败数/率、输入、新增输入、输出、缓存读、延时、首字、速度、成本；缓存 chip 的 title 带缓存写与命中率，成本 chip 的 title 带覆盖率。
- hover 任意 lane 时切换成该桶的同一组数字，并在标题旁标「进行中」（最后一桶）。
- 汇总数据来自 `summaries.find(s => s.model_id === modelId)`，不从桶数据重算；矩阵失败时显示 —（`modelMatrix` 失败块已在 failbar 点名）。
- 首字在 `stream_requests === 0` 时显示 —；成本用 `formatCost` + `formatCostCoverage`；token 用 `formatTokenCount`。
- 读数条同时充当 screen reader 的「一眼读数」，sr-only 表仍是逐桶的完整替代。

### 6.5 空态 / 失败态

- 模型在本时段无请求：面板内一行「该模型在本时段没有请求」+「切回全部模型」按钮；不画空 lane。
- `tsData` 失败：`PanelFailed`（与现有一致），不渲染旧数据。
- 选中模型不在选项（切时段后消失）：把选中值补进选项；面板显示上面的空态。
- 全部模型视图无数据：保持现有空态，不显示模式切换。

### 6.6 文案

新增 key（zh / en 各一份，能复用 `mShort*` / `col*` 的就复用）：

| key | zh | en |
|---|---|---|
| `trendModeAll` | 全部模型 | All models |
| `trendModeSingle` | 单模型 | Single model |
| `singleModelSelect` | 选择模型 | Select model |
| `singleModelEmpty` | 该模型在本时段没有请求 | No requests for this model in this range |
| `bucketInProgress` | 进行中 | in progress |
| `partialBucketNote` | 最后一桶尚未结束，读数会继续增长 | The last bucket is still in progress |
| `timelineTzNote` | 小时桶按本地时区，日/月/年按 UTC | Hour buckets use local time; day/month/year use UTC |
| `ttftNoSamples` | 无流式样本 | No streaming samples |
| `unpricedModel` | 该模型未定价，成本不可估算 | This model has no prices; cost cannot be estimated |
| `unknownModel` | 未知（后端已删除） | Unknown (backend deleted) |
| `cacheWrite` | 缓存写 | Cache write |
| `zeroFailuresNote` | 本期 0 失败 | No failures in this range |
| `zeroCacheNote` | 本期无缓存命中 | No cache hits in this range |
| `zeroStreamNote` | 本期无流式请求 | No streaming requests in this range |

- 不硬编码任何标签；模型名、枚举值保持原样。

### 6.7 响应式

- ≥1280：5 条 lane 全宽；每条 84–96px；面板总高约 480–560px（全屏 1.2×）。
- <1280：lane 高度降到 72–84px；摘要条换行；x 刻度抽稀（沿用现有 `interval` 逻辑）。
- <768：模型 Select 独占一行；tooltip 宽度收到 240px；lane 只显示 3 个 x 刻度。
- 不新增横向滚动；`ResponsiveContainer` 必须有确定高度（沿用 `.wall-trend-panel` 的做法）。

---

## 7. 验收与测试

### 7.1 功能验收

- [ ] 页头切时段/粒度，全部模型与单模型两种模式的桶数、边界一致。
- [ ] 切模型瞬时，不重新请求；摘要条与矩阵同一模型行逐项相等。
- [ ] 请求为 0 的桶：计数为 0，耗时/首字/速度为断点，tooltip 显示 —。
- [ ] 未定价：成本线断开，摘要条 —，有「未定价」说明；不能出现 0。
- [ ] 只有非流式请求的桶：首字断点，tooltip 说明无流式样本。
- [ ] 失败高峰：失败率线可见，失败数在 tooltip；与矩阵失败数一致。
- [ ] 最后一桶：tooltip 带「进行中」，脚注存在。
- [ ] 模型在本时段无请求：空态，不自动切模型。
- [ ] `tsData` 失败：failbar 点名「模型请求统计」，面板显示失败，不显示 0/旧值。
- [ ] 明暗两套、1280/1440/1920 无横向溢出、无裁切。

### 7.2 数据不变量

- `Σ bucket.requests = summary.requests`
- `Σ bucket.failed = summary.failed_requests`
- `Σ bucket.token_input / fresh / cache_read / output = summary` 对应字段（`ModelSummary` 没有 cache_write 字段，cache_write 只做桶内自洽）
- `Σ bucket.cost = summary.total_cost`；`Σ bucket.priced_requests = summary.priced_requests`
- `Σ(avg_latency × requests) / Σ requests = summary.avg_latency_ms`（容差 0.01）
- 不变量做成 Rust 单测（同一 SQL 范围）；E2E 只做抽样断言，避免数据库噪声。

### 7.3 E2E

新增 `e2e/test_model_timeline.py`（或并入 `test_wall.py`）：

1. 专用实例 + 直接写 `request_stats` 固定时间/模型/token/cost 的行（`test_visual.py` 已有独立实例与固定种子库的模式）；覆盖 3 个桶：有流量、0 请求、只有非流式；两个模型。
2. 打开 `/wall`，切「单模型」，断言 lane 数、桶数、摘要条与固定值。
3. 矩阵模型名按钮 → 模式切换 → 选中；断言 `aria-label`、键盘可达。
4. 切到另一个模型，断言数值变化且摘要条跟着变。
5. 空桶断言：计数 0、均值列在 sr-only 表里是 —（不是 0）。
6. 未定价模型：成本 lane 不出现/有说明；成本列 —。
7. 失败场景：`page.route` abort `/api/stats/model-timeseries`，断言 failbar 点名、面板不显示 0。
8. 布局探针扩展：每条 lane 高度 > 60；所有 lane 的左右边界一致；无横向溢出。

- 新增 spec 已实现 3 条：固定种子数据的读数/切换/空桶、取数失败、三宽度 × 亮色布局 + axe；暗色 axe 与键盘焦点移动在第一、三条里断言。现有 `e2e/test_a11y.py` / `test_keyboard.py` 继续覆盖 `/wall` 默认视图。
- 全量 E2E 现在是 25 条（原 22 + 新 3），约 95s。
- `e2e/test_visual.py`：如果模式切换只在有数据时渲染，零流量的 `/wall` 基线不变；若加了空态控件，需 `UPDATE_BASELINES=1` 重生。
- 现有 `test_wall.py` 的布局探针要加 lane 相关检查；失败场景断言「no pageerror」，沿用现有写法（abort 本身会留下 `net::ERR_FAILED` 资源日志）。

### 7.4 性能

- 后端：同一查询多 4 个聚合，无额外扫描；30 天 × 日 × 20 模型实测响应 < 100ms（本地库）作为目标线，实测值记在 PR 里。
- 前端：单模型派生是 O(桶数 + 点数)，168 桶无感；切换不发请求。
- 30s 自动刷新期间不重置模型选择、不闪加载遮罩（沿用 `silent`）。

---

## 8. 实施顺序

| 阶段 | 内容 | 验收 |
|---|---|---|
| P1 数据 | 结构体 + SQL + Rust 单测 | `cargo test`；现有趋势图无变化；新字段有值 |
| P2 核心 | 模式切换 + 模型选择 + 流量/延迟/Token 三条 lane + 摘要条 | 时间桶与全部模型一致；空桶/未定价语义正确 |
| P3 完整 | 速度/成本 lane + 悬停读数条 + sr-only 表 + 空/失败态 | 10 个指标同屏可读；a11y 表完整 |
| P4 验收 | 文案/响应式/矩阵入口 + E2E + axe + 键盘 + 像素 | 全量 E2E 通过；基线按需更新 |
| P5 可选 | p95/成功率拆线、按平台拆分、成本折扣、指数视图 | 另行排期，不阻塞 P1–P4 |

每阶段独立可发布；P1 不改变 UI，可先合。

验证命令：

```bash
cargo build
cargo test
npm --prefix frontend run build
python -m pytest e2e -q
```

E2E 前置产物与端口约束见 `e2e/conftest.py`；`/wall` 的像素基线在 `.dsh-tmp/visual/`（gitignored），设计确认后用 `UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py` 重生成。

---

## 9. 需要拍板的点（附建议）

1. **单图 vs 分面**：建议分面（5 条共享 x 的小图）；如果坚持一张图，先只保留同量纲的 2–3 条，其余进 tooltip。
2. **成本 lane**：建议默认展示（有已定价请求时）；未定价时用文字说明，不画空图。
3. **平滑**：建议 `monotone` + 桶少自动 `linear`；不做隐式移动平均。如果你说的「平滑」是指移动平均，请明确窗口。
4. **时间桶时区**：建议本期只写清楚（小时本地 / 日 UTC），不夹带口径迁移。
5. **速度口径**：建议沿用现有「有效吞吐」（含失败耗时），另开「成功请求速度」作为后续新指标。
6. **入口**：建议矩阵模型名按钮 + 面板模式切换；不做整行点击、不做新路由。

---

## 10. 一句话总结

把「同时看 10 个指标」拆成「同一桶的 10 个数字同时出现」，把「一张图」拆成「共享 x 轴的 5 条小图」，把「时间一致」落实成「复用同一份 timeseries 响应」——这样既满足你要的视野，又不会用一根 y 轴把成本和延迟画成两条假的水平线。

---

## 11. 实现状态（2026-10-25）

- P1–P4 已落地：后端 5 个聚合字段、单模型模式、矩阵下钻、5 条 lane、悬停读数条、sr-only 表、空/失败/未定价语义、三宽度与明暗 axe、像素基线未变。
- 验证：`cargo test` 109 个 lib 测试通过；`python -m pytest e2e -q` 25 passed（约 95s）；`test_visual.py` 3 passed。
- 与计划的两处实现取舍：用「共享悬停读数条 + 跨 lane 竖线」代替 5 个浮动 tooltip；稀疏均值序列在孤立点补点、关闭入场动画。两者都是为了在大屏和 30s 刷新下读得准、截得稳。
