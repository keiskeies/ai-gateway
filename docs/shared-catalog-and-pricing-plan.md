# 共享模型目录与分时计费 · 设计与落地计划

> 目标：同一 `platform_type + base_url` 的多个账号（不同 API Key）共享一份模型目录、价格与积分单价；计费支持 DeepSeek 式「缓存命中输入 / 缓存未命中输入 / 输出」以及「高峰 / 空闲时段」价格。
>
> **结论先行**：不建 per-account 的模型副本，也不建模板表；直接把 `models` 升级为**共享目录**：`catalog_key` 由 `type + 规范化 base_url` 哈希得到（不含 API Key），`models` 唯一键从 `(platform_id, model_id)` 扩展为 `(catalog_key, model_id)`。`platform_id` 只表示“这条目录行登记在哪个账号下”，价格/积分单价天然全账号共享；积分预算仍在 `platforms`，按账号独立。

---

## 1. 共享目录

### 1.1 catalog_key

```text
catalog_key = sha256( lowercase(platform_type) + "|" + normalize(base_url) )
```

- 不含 API Key。
- `normalize`：trim、去掉末尾 `/`、scheme 与 host 小写；path 保留大小写。
- `platform_type` 参与哈希：同一 URL 用 OpenAI 兼容 / Anthropic 协议接入时目录不混。
- 示例：`https://api.deepseek.com/v1` 与 `https://API.deepseek.com/v1/` 得到同一个 key；换 Key 不变。

### 1.2 数据模型（V25）

```sql
ALTER TABLE platforms ADD COLUMN catalog_key TEXT;
ALTER TABLE models ADD COLUMN catalog_key TEXT;
ALTER TABLE models ADD COLUMN cache_read_price REAL;
ALTER TABLE models ADD COLUMN cache_write_price REAL;
ALTER TABLE models ADD COLUMN peak_pricing TEXT;

CREATE INDEX IF NOT EXISTS idx_platforms_catalog ON platforms(catalog_key);
-- 去重后再建：CREATE UNIQUE INDEX idx_models_catalog_model ON models(catalog_key, model_id);
```

- `models.platform_id` 保留，语义变为「登记账号」：只用于目录行的归属/删除时的存活处理；**价格读取不再按 platform_id JOIN**。
- `models` 的唯一键：旧的 `(platform_id, model_id)` 保留，新增 `(catalog_key, model_id)` 唯一索引。
- 迁移顺序（必须）：
  1. 加列；
  2. Rust 回填 `platforms.catalog_key`；
  3. `models.catalog_key = platform.catalog_key`；
  4. 按 `(catalog_key, model_id)` 去重：保留“非空价格字段最多”的那行，平手保留最早平台的行；
  5. 建唯一索引。
- 现有重复模型行删除是安全的：`backends.model_id` 存的是上游模型名字符串（V5），不引用 `models.id`。

### 1.3 读写语义

| 操作 | 语义 |
|---|---|
| 平台列表/详情 | 返回 `catalog_key`；第二个同 URL 账号的模型数 = 共享目录的模型数 |
| 模型列表 | `list_by_platform` 先解析平台 catalog_key，再按 catalog_key 查；第二个账号无需重新拉取 |
| 模型同步/导入 | `import_model_ids` 以 catalog_key 判重；新模型写入共享目录，所有同 catalog 账号立刻可见 |
| 模型编辑/批量改价/CSV 改价 | 直接改共享目录行；改一处全账号生效 |
| 模型删除 | 删除共享目录行；同 catalog 账号全部失去该模型 |
| 平台删除 | 若被删平台是目录行的 `platform_id` 登记账号，先把这些行改挂到同 catalog 的兄弟账号；没有兄弟则随外键级联删除 |
| 平台改 base_url/type | 重算 catalog_key；旧目录若无人引用则清理孤儿目录行，不自动迁移模型 |
| 积分预算 | 仍在 `platforms`，按账号独立；模型 `credit_price` 在共享目录，全账号一致 |

### 1.4 路由读取

`db/route.rs` 的价格 JOIN 改为：

```sql
FROM backends b
LEFT JOIN platforms p ON p.id = b.platform_id
LEFT JOIN models m ON m.catalog_key = p.catalog_key AND m.model_id = b.model_id
```

`Backend` 额外携带 `cache_read_price`、`cache_write_price`、`peak_pricing`。

---

## 2. 分时 + 缓存计费

### 2.1 价格字段

- `input_price`：缓存**未命中**输入价（旧字段，语义收窄，兼容旧数据）。
- `cache_read_price`：缓存**命中**输入价；NULL → 回退 `input_price`。
- `cache_write_price`：缓存写入价；NULL → 回退 `input_price`。
- `output_price`：输出价。
- 以上均为每 100 万 token，与现有口径一致。
- `peak_pricing` JSON：

```json
{
  "windows": [{ "start": "08:30", "end": "17:30" }],
  "peak": {
    "input": 2,
    "cache_read": 0.04,
    "cache_write": null,
    "output": 8
  }
}
```

- 基础价视为默认/空闲价；`windows` 命中时用 `peak` 覆盖；`peak` 里缺的字段回退基础价。
- 多个窗口用数组；支持跨午夜（`22:00-02:00`）。
- 时段按**服务器本地时区**判断；请求开始时取一次时间，长流跨过边界不换价。
- `windows` 为空或 `peak` 为 null → 始终用基础价。

### 2.2 成本公式

```text
hit      = cache_read_tokens
write    = cache_write_tokens
fresh    = max(0, token_input - hit - write)
price    = tier(now)  // 高峰覆盖或基础价
cost     = (fresh*price.input + hit*price.cache_read + write*price.cache_write
            + token_output*price.output) / 1_000_000
```

- `token_input` 或 `token_output` 缺失 → 整单 NULL（保持现有“宁可不估”的口径）。
- 缓存 token 缺失 → 按 0；旧模型未配 cache 价时，hit/write 回退 input_price，公式退化为旧口径。
- 成本仍在写入统计行时冻结；历史行不回填。
- 流式请求用请求开始时的 `pricing_at`；非流式同样。

### 2.3 DeepSeek 对应关系

以截图为例（DeepSeek-V4.1-Flash，每 1M token）：

| 字段 | 空闲（基础价） | 高峰覆盖 |
|---|---:|---:|
| 缓存命中输入 `cache_read_price` | 0.02 | 0.04 |
| 缓存未命中输入 `input_price` | 1 | 2 |
| 输出 `output_price` | 4 | 8 |
| 缓存写入 | 无（NULL） | 无（NULL） |

高峰窗口按实际 DeepSeek 公布时段填入；工具不猜时段。

---

## 3. 代码落点

| 文件 | 改动 |
|---|---|
| `src/catalog.rs`（新） | normalize / catalog_key / 单测 |
| `src/models/pricing.rs`（新） | `PricingWindow` / `PeakPricing` / `Pricing` / 成本公式 / 校验 / 单测 |
| `src/db/schema.rs` | V25：加列、回填、去重、唯一索引；`CURRENT_SCHEMA_VERSION=25` |
| `src/models/platform.rs` | `Platform.catalog_key` |
| `src/db/platform.rs` | 创建/更新算 key；删除时把目录行改挂兄弟账号；清理孤儿目录 |
| `src/models/model.rs` | `catalog_key`、cache/peak 字段；CRUD/批量/CSV 请求字段 |
| `src/db/model.rs` | 查询/创建/更新/批量/CSV 按 catalog_key；`list_by_platform` 解析 key；去重/孤儿清理 |
| `src/db/route.rs` | 价格 JOIN 改 catalog_key；读取 cache/peak |
| `src/models/route.rs` | `Backend` 新字段；`pricing_snapshot()` / `estimate_cost_detailed()` |
| `src/proxy/handler.rs` | 请求开始取 `pricing_at`；非流式/流式都按 token 明细计费 |
| `src/api/platform.rs` | 同步/导入/CSV 共享目录；平台响应含 catalog_key |
| `src/api/model.rs` | peak_pricing 校验 |
| `src/models/config_transfer.rs`、`src/db/config_transfer.rs` | 新增 `catalog_key`/cache/peak 字段；导入去重、回填 |
| `frontend/src/pages/ProviderDetail.tsx` | 模型按 catalog_key 过滤；缓存价/高峰价编辑；共享提示 |
| `frontend/src/pages/Platforms.tsx` | 模型数按 catalog_key |
| `frontend/src/types/stats.ts`、`src/api.ts`、`src/i18n.ts` | 类型/文案 |
| `src/lib.rs`、`src/models/mod.rs` | 注册新模块 |

---

## 4. 边界与取舍

- 同一 URL 的账号共享模型目录：某个账号无权限的模型也会显示（并集）。本次不做账号级可用性开关。
- 高峰时段按服务器本地时区；跨时区部署需后续加 timezone 字段。
- CSV 本次只扩展 cache_read/cache_write 两列；高峰 JSON 走模型编辑 UI，不塞进 CSV（避免在表格里手写 JSON）。
- 删除共享模型会影响同 catalog 的所有账号；UI 文案需明确。
- `platform_id` 仍是目录行的登记账号；它不再代表“价格/模型归属”，只用于删除存活与配置导出。
- 积分预算按账号独立，不随目录共享。

---

## 5. 二次验证记录

| # | 初稿风险 | 核对/修正 |
|---|---|---|
| 1 | 用模板表 + 克隆/传播 | 改为 `models` 本身共享（catalog_key 唯一）；少一层同步，天然改一处全账号生效 |
| 2 | 直接建 `(catalog_key, model_id)` 唯一索引 | 旧库可能已有同 URL 多账号的重复行，必须先回填、去重，再建索引 |
| 3 | 价格 JOIN 继续按 `m.platform_id = b.platform_id` | 第二个账号没有模型行，必须改为 `m.catalog_key = p.catalog_key` |
| 4 | 前端按 `model.platform_id === platform.id` 过滤 | 共享行登记在首个账号下，必须改为按 `catalog_key` 过滤 |
| 5 | 删除首个账号会级联删掉共享模型 | 删除前把 `models.platform_id` 改挂到同 catalog 的兄弟账号 |
| 6 | 把缓存 token 再当普通输入计价 | 公式用 `fresh = total - cache_read - cache_write`，缓存价独立 |
| 7 | 旧模型没有 cache 价 | cache_read/cache_write 回退 input_price，旧成本口径不变 |
| 8 | 高峰窗口跨午夜 | 窗口解析支持 `start > end` 的环绕；开始含、结束不含 |
| 9 | 长流跨高峰边界换价 | 请求开始取一次 `pricing_at`，整单用同一个时段 |
| 10 | 历史成本被改价影响 | 成本写入时冻结，继续不回填 |
| 11 | 配置导入带重复共享模型 | 按 catalog_key 去重/忽略，并回填 catalog_key |
| 12 | 平台改 URL 留下孤儿模型 | 更新后清理 `catalog_key` 不在 platforms 里的目录行 |

---

## 6. 测试

1. `catalog_key`：大小写/末尾斜杠归一；Key 不同不影响；type 不同 key 不同。
2. V25：回填、去重、唯一索引；重跑幂等。
3. 共享目录：同 URL 第二个账号立即看到模型；改价/改模型全账号生效；同步新模型全账号可见；积分预算仍各自独立。
4. 删除：删除登记账号后目录行改挂兄弟；无兄弟时删除。
5. 计费：缓存命中/未命中/写入分别计价；高峰窗口命中/不命中/跨午夜；缺字段回退；旧价格口径不变。
6. 路由：第二个账号的 backend 能通过 catalog_key 读到共享模型价格。
7. 前端：模型编辑缓存价/高峰价；第二个账号模型列表无需重新拉取；构建与 axe 检查。
8. 集成：mock 上游返回 cached_tokens + 高峰时间，验证统计 cost。
