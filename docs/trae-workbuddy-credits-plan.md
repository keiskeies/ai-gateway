# Trae / WorkBuddy 订阅积分接入 + Key 申请快捷入口 · 设计与落地计划

> **配套材料**：一手调研原文见 [`docs/research/`](research/README.md)（`trae.md` / `workbuddy.md` / `api-key-links.md`），保留全部来源 URL 与实测状态码，供实现时核对。

---

> **一句话**：把「Trae / WorkBuddy 的订阅积分」当成一种**新的上游类型**接进现有路由与积分台账，把「Agnes / 魔搭」当成**普通 OpenAI 兼容平台**零改动接入，再给所有平台配一张**可点击的 Key 申请入口表**。
>
> **但读完调研后，这个「一句话」需要加一个前提**：Trae / WorkBuddy 的订阅积分接入**明确违反其服务条款**（原文已核实，见 §10.1），且 WorkBuddy 上游正在主动封堵（§2.6）。因此本文档的推荐路径是 **B + C + A2（官方通道）**，逆向反代（A3）作为「用户明确知情后的可选路径」，完整设计仍保留在 §3~§9。

---

## 0. 结论先行

### 0.1 三件事，三种性质，不能混在一个方案里

用户的一句话里其实混了三个完全不同性质的诉求，混着做必然做错：

| # | 诉求 | 本质 | 官方支持度 | 工作量 | 风险 | 稳定性 |
|---|---|---|---|---|---|---|
| **A** | 用 Trae / WorkBuddy 的**订阅积分**调模型 | 把「客户端订阅额度」逆向成 API | ❌ 无公开 API，靠客户端私有协议 | 大（新上游适配层 + 凭据保险箱） | **🔴 明确违反 ToS**（已核实原文，见 §10） | **低**（上游在主动封堵，见 §2.6） |
| **B** | 用 Agnes / 魔搭的 **API Key** 调模型 | 标准 OpenAI 兼容 API | ✅ 官方文档明确支持 | **零**（加预设即可） | 低 | **高** |
| **C** | **快捷跳转**到各家的 API Key 申请页 | 纯前端外链注册表 | — | 很小 | 无 | **高** |

**落地顺序必须是 B → C → A。** B 和 C 一天能做完，且立刻可用、不会失效；A 是真正的工程。

### 0.1.1 🔴 关于 A 类，必须先纠正一个认知（本方案最重要的结论）

动手前我原以为 A 类属于「社区普遍接受的灰色地带」。**核实 ToS 原文后，这个判断是错的。**

Trae 的《用户协议》（更新时间 2026/06/05，通过官方渲染接口 `POST api.trae.com.cn/cloudide/api/v3/trae/terms/get` 取得全文，我已亲自验证）明确列出：

| 条款 | 原文 | 对应反代行为 |
|---|---|---|
| **5.3(9)** | 「对本服务进行**逆向工程、拆卸、反向汇编、反编译**……或以其他方式试图发现……底层组件、底层技术、算法、模型」 | 读 `storage.json`、解密 `tc` 凭据、逆向私有协议 |
| **5.2** | 「**使用任何自动化程序、软件或类似工具接入本服务**」 | 网关自动转发请求 |
| **5.3(14)** | 「**以自动化的方式使用本服务**……或超出我们的任何速率限制」 | 同上 |
| **5.3(11)** | 「**出租、租赁、分发、许可、转让或出售**……访问或使用本服务或其任何部分」 | 把额度转成 API 供其它工具使用 |
| **5.3(16)** | 「**绕过我们可能用于防止或限制访问本服务的任何措施**」 | 伪造 UA / 设备指纹 |
| **4.4** | 「账号仅限于你本人使用……**禁止以任何形式赠与、借用、出租、转让、售卖**」 | 多账号池 |
| **5.4** | 「有权不事先通知你……**警示、限制功能、暂停或终止向你提供服务**」 | 处置后果 |

国际版 ToS（Last Updated: September 17, 2026，`POST ug-normal.trae.ai/cloudide/api/v3/trae/terms/get`）Section 5 Prohibited Uses 同样明文禁止：「**autonomously or programmatically extract output**」「**rent, lease, distribute, license, transfer, or sell the Services**」「**reverse engineer, disassemble, decompile**」。

**定性**：反代 = 逆向工程 + 自动化接入 + 分发额度，**三重明确违约，没有灰色空间**。

> 这与 Agnes / 魔搭形成鲜明对比：后者是「官方提供 API、官方文档教你怎么用、按量付费」。**同样写在代码里，法律性质完全不同。**

### 0.1.2 那 A 类到底还做不做？—— 三条路，让用户选

我不替用户做价值判断，但必须把选项和代价摆清楚：

| 选项 | 做法 | 合规 | 工程量 | 建议 |
|---|---|---|---|---|
| **A1 不做** | 只做 B + C | ✅ | 1 天 | **推荐作为首发** |
| **A2 做，但走官方通道** | Trae 用 **TRAE CLI 2.0 + ACP**（`traecli acp serve`，stdio 协议，官方文档支持）；WorkBuddy 用**官方自定义模型**（反方向：让 WorkBuddy 调网关） | ✅ 合规 | 中（ACP↔OpenAI 适配层） | **推荐作为第二阶段** |
| **A3 做，逆向反代** | 本文档 §3~§9 的完整设计 | 🔴 违约 | 大 | 需用户明确知情并自担风险 |

**本文档的主体（§3~§9）是 A3 的完整设计**——因为用户明确要求了这个功能。但：
- 它**默认关闭**、不预置、需用户在知情弹窗里确认 ToS 风险后才出现
- 它**不实现**任何风控规避（见 §10.2）
- **A2 是更值得先做的**：同样能「用上 Trae/WorkBuddy 的额度」，但不违约

> **给用户的一句话**：如果你的目标是「省下 API 费用」，A2 完全够用且不违约；A3 换来的是「标准 OpenAI 接口」，代价是账号风险 + 违约。这个取舍应该由你决定，而不是我替你默认。

> **A 类内部排序**：Trae 的生态仍活跃（项目在 2026-10 仍在更新，**未见 WorkBuddy 式整体封堵**），WorkBuddy 的两个主力项目已在 2026-10 因上游封堵停止/降低维护。所以若做 A3，**Trae 先做，WorkBuddy 标为实验性并延后**。

### 0.2 三个产品身份已核实（这是本方案的地基）

用户口语与实际产品的对应关系（全部实测确认，见 §2）：

| 用户原话 | 实际产品 | 厂商 | 国内站 | 海外站 |
|---|---|---|---|---|
| trae | Trae（TraeWork / TraeCode / TRAE SOLO） | 字节跳动 | `trae.cn` | `trae.ai` |
| workbuddy | WorkBuddy（与 CodeBuddy 同属 Buddy AI，**积分共享**） | 腾讯 | `workbuddy.cn` | `workbuddy.ai` |
| **agens** | **Agnes AI**（拼写是 a-g-n-**e**-s） | Agnes Tech | `agnes-ai.cn` | `agnes-ai.com` |
| 魔塔社区 | **魔搭**社区 ModelScope | 阿里巴巴 | `modelscope.cn` | — |

> **「agens」的判定证据**：本仓库已有 `agnes` 的痕迹——`src/protocol/openai.rs:437`「部分上游（如 agnes）要求 user/system/tool 消息必须带 content 字段」、`src/protocol/content.rs:128`「agnes 的 vLLM 部署无法处理单通道图片」、`src/proxy/handler.rs:1077` 同一条；文档里出现过模型名 `agnes-3.0-flash`、`agnes-5`。而 `agnes-ai.com` 的官方文档里正好有 `agnes-3.0-flash`、`agnes-2.5-flash`、`agnes-5` 系列。这不是巧合。

### 0.3 关键设计洞察：多账号池不需要新建一套机制

B 类上游（Trae / WorkBuddy）是**一个账号一份额度**，用户显然会有多个账号（国内号 + 海外号）。直觉上要新建「账号池」表。

**不需要。** 现有 V25 的 `catalog_key = SHA256(type_key | normalize_base_url)` 已经实现了「同一 Base URL 的多个平台行共享一份模型目录与价格」，而 `BackendSelector` 已经会在同一路由的多个后端之间轮转，`platform_credit_state` 已经按 `platform_id` 独立记账。

所以：

> **一个上游账号 = 一行 `platforms`。多个账号 = 同一个 `catalog_key` 下的多行。**
> 模型目录共享、价格共享、路由轮转、积分独立记账、耗尽自动跳过——**全部是现成的**。

这条决定了整个方案的体量：A 类不需要「账号池子系统」，只需要「凭据保险箱 + 上游适配器 + 额度同步」三个增量。

---

## 1. 范围与不做的部分

### 1.1 本次做

| 编号 | 内容 |
|---|---|
| **P1** | `PlatformAdapter` 枚举：在现有 `OpenAI` / `Anthropic` 之外增加 `Trae` / `WorkBuddy` 两条私有协议路径 |
| **P2** | `platform_credentials` 表（V28）：加密存放 accessToken / refreshToken / uid / enterpriseId / 到期时间，与 `platforms.api_key` 分离 |
| **P3** | 凭据刷新：后台任务 + 请求前惰性刷新；`ExchangeToken` 会轮换令牌族的坑用「默认不自刷」开关规避 |
| **P4** | 上游额度同步：`POST {billingBase}/v2/billing/meter/get-user-resource`（WorkBuddy）/ Trae `notify_usage` → 校准 `platform_credit_state` |
| **P5** | 错误码映射：`4008` / `1005` / `4001` / `4017` / `4023` → 网关语义 + 冷却策略 |
| **P6** | Key 申请入口注册表：`GET /api/platforms/key-links` + 前端「申请 API Key」按钮 |
| **P7** | Agnes / 魔搭等新增平台预设（含国内 / 海外双站） |
| **P8** | 前端：平台创建弹窗的「上游类型」选择、凭据表单、额度读数、快捷链接 |

### 1.2 本次不做

| 不做 | 理由 |
|---|---|
| Trae 的 `create_agent_task` 完整 Agent 通道 | 请求体 180–210 KB，含未复刻的 `encrypted_prompt_set` 加密参数；社区项目至今未打通。本方案只用 `llm_utils_chat` 纯对话通道 |
| 第三方模型渠道名（`<渠道>//<模型名>`） | 裸通道不认这些名字（`4001` / `4023`），需要官方客户端云端 agent 通道 |
| 自动签到 / 自动领额度 | 这是「薅羊毛」行为，明确越界，不做 |
| 绕过风控（设备指纹伪造、IP 轮换） | 同上，不做。`4017` 只做「长冷却 + 告警」，不做规避 |
| 图片 / 视频模型 | Agnes 的 image / video 走 OpenAI Videos 兼容 API，与当前网关的 chat 主链路不同，另立需求 |
| 多租户共享额度 | 与项目「本机控制台」定位冲突（`/api/clients` 已限制 loopback） |

---

## 2. 事实核对（落地前必须承认的上游现实）

> 以下每一条都有实测来源。**凡标「社区实测」的都不是官方文档**，会随客户端版本变化，代码里必须留降级路径。

### 2.1 Agnes AI（官方 API，B 类，零风险）

| 项 | 值 | 来源 |
|---|---|---|
| 海外官网 | `https://agnes-ai.com` | 200 |
| 国内官网 | `https://agnes-ai.cn` | 200 |
| 海外控制台（**Key 申请**） | `https://platform.agnes-ai.com/settings/apiKeys` | 200（精确路径，见下） |
| 国内控制台（**Key 申请**） | `https://platform.agnes-ai.cn/settings/apiKeys` | 200（独立域名，实测） |
| 文档站（海外） | `https://wiki.agnes-ai.com` + `/llms.txt`、`/_llms/zh-hans.txt` | 200 |
| 文档站（国内） | `https://wiki.agnes-ai.cn` + `https://agnes-ai.cn/doc/*` | 200 |
| **海外 OpenAI 兼容 base_url** | `https://apihub.agnes-ai.com/v1` | 官方 overview.md / quickstart.md |
| **国内 OpenAI 兼容 base_url** | **`https://api.agnes-ai.cn/v1`** | 国内站官方 overview.md 明写 |
| 鉴权 | `Authorization: Bearer YOUR_API_KEY` | 官方文档 |
| 无 Key / 错 Key 时 | `/v1/models` 与 `/v1/chat/completions` 均 **401** | 实测（两个 hub 行为一致） |
| 端点 | `/chat/completions`、`/responses`、`/messages`；图像 `/images/generations`、视频 `/videos` | 官方模型页 + FAQ |
| 自定义 header | **无**，标准 Bearer | 官方文档未提及任何自定义头 |
| 错误格式 | 标准 OpenAI：`{"error":{"message","type","param","code"}}` | 实测 |
| 文本模型 | `agnes-2.5-flash`、`agnes-2.5-pro`、`agnes-2.5-pro-beta`、`agnes-3.0-flash`、`agnes-3.0-pro` | llms.txt |
| 图像 / 视频 | `agnes-image-2.x-flash`、`agnes-video-2.5`（OpenAI Videos 兼容） | llms.txt |
| Token Plan | `.../zh-Hans/docs/tokenplan.md`（访问计划、RPM 限制、订阅配额） | llms.txt |
| 定价 / 错误码 | `.../pricing.md`、`.../code.md` | llms.txt |

**结论**：Agnes 是标准 OpenAI 兼容上游，**`platform_type = OpenAI`，零代码改动**。

**国内 / 海外是两套完全独立的站**，base_url **不同**（这是本次调研最重要的一条）：

| | 海外站 | 国内站 |
|---|---|---|
| Key 申请页 | `platform.agnes-ai.com/settings/apiKeys` | `platform.agnes-ai.cn/settings/apiKeys` |
| **base_url** | `https://apihub.agnes-ai.com/v1` | **`https://api.agnes-ai.cn/v1`** |
| 计价 | 美元 / Token Plan（按**请求次数**配额） | 人民币 / Agent Plan（按 **Credits 积分**） |

所以网关里配 **两行平台**，base_url 不同 → `catalog_key` 天然不同 → 模型目录、价格、积分台账全部独立 ✅。

> **注意一个易错点**：`apihub.agnes-ai.cn` 也能解析（我实测返回 401），但**官方文档从未提及**，属实现细节。配置里必须写官方口径的 `https://api.agnes-ai.cn/v1`，不要写 `apihub.agnes-ai.cn`。

> **Agnes 特有的错误码（网关必须单独处理）**：额度耗尽返回 **402 而非 429**（官方错误码页明写）。现有网关的重试策略是按 429/5xx 匹配的，402 会被当成「永久失败」直接透传给客户端——这在语义上是对的，但**不会触发换后端**。需要在 `error_map` 里把 402 映射为 `QuotaExhausted`，让它参与换号与积分台账，而不是直接失败。

> **另一个易错点**：官方 404 的成因里明确列了「`/v1` 被第三方工具重复添加」。现有 `src/catalog.rs::normalize_base_url` 已经会**去掉末尾斜杠**并把 scheme/host 小写化，所以 `https://api.agnes-ai.cn/v1/` 能正确归一。但它**不会**自动补 `/v1`——这是对的，因为有的上游 base 就不带 `/v1`（如 Anthropic 的 `https://api.anthropic.com`）。转发路径由 `build_forward_request` 拼 `{base_url}/chat/completions`，所以用户填错 base_url 时会得到上游的 404，错误信息里会带真实路径，便于排查。

### 2.2 魔搭 ModelScope（官方 API，B 类，零风险）

| 项 | 值 | 来源 |
|---|---|---|
| Key / Access Token 页 | `https://www.modelscope.cn/my/myaccesstoken` | **200** |
| 推理 base_url | `https://api-inference.modelscope.cn/v1` | 官方文档 |
| 文档页 | `https://www.modelscope.cn/docs/model-service/API-Inference/intro` | 200 |
| **`/v1/models` 无 Key 可列** | **200**，返回 **37 个模型** | 实测 |
| 模型 id 命名 | `deepseek-ai/DeepSeek-V4-Pro`、`deepseek-ai/DeepSeek-V4.1-Flash-1017`、`Qwen/Qwen3-*` 等（`owner/Model` 形式） | 实测 |

**结论**：同样是零改动接入。注意模型 id 含 `/`，`GET /api/platforms/{id}/remote-models` 的同步要能处理带斜杠的 id。

### 2.3 Trae（官方定价已核实 + 社区实测协议）

**官方积分机制（已核实，来源 `https://www.trae.cn/pricing` 与 `https://www.trae.ai/pricing`，两份我都实测抓取）**：

| | 国内站 `trae.cn` | 海外站 `trae.ai` |
|---|---|---|
| 计量单位 | **积分**（credits） | **Usage（美元额度）** |
| 免费档 | Free ¥0，**500 积分/月** | Free $0，仅 Auto 模式、有限用量 |
| 付费档 | Lite ¥49（2,000）、Pro ¥99（4,000）、Pro+ ¥239（12,000）、Ultra ¥699（40,000） | Pro $20/月（$20 usage）、Pro+ $60（$60）、Ultra $200（$200） |
| 额外积分 | **会员签到多得 3,000 积分** | — |
| 模型折扣 | GLM 5.3 / Seed-2.1-Turbo / Seed-Code / DeepSeek-V4.1-Flash / V4-Pro 有 2.5~5 折 | — |
| 并发云端任务 | 2 / 2 / 10 / 10 / 20 | 2 / 10 / 15 / 20 |

**积分机制的关键细节（官方文档 `docs.trae.cn/ide_plans-and-billing`）**：

- 积分是 **31 天有效期的钱包**，**不是**「每日/每月刷新的配额」——官方没有「每天重置额度」这回事
- 免费用户：每月登录 500（当月有效）+ 注册一次性 2000（31 天）+ **每日签到 100** + 邀请各 500
- 会员：套餐积分（31 天）+ 每月登录 500 + **每日签到 200** + 邀请 500
- **只有内置模型消耗积分；自定义模型（BYOK）不消耗**
- 消耗顺序：先烧剩余有效期最短的 → 同有效期奖励积分优先于会员积分
- 增购 ¥50/1000 积分（31 天有效）
- 消耗倍率按**忙时 8:00–22:00 / 闲时 22:00–次日 8:00 自动 5 折**；示例倍率：DeepSeek-V4-Pro 0.72、GLM-5.3 0.78、Seed-2.1-Turbo 0.4、DeepSeek-V4.1-Flash 0.12

> **这是本方案最重要的一个设计输入**：Trae 国内站是「**积分**」，海外站是「**Usage（美元额度）**」——**两者不是同一个计量单位**。
>
> 更关键的是：**积分是「31 天钱包」，不是「自然月配额」**。而本项目的 `platform_credit_state` 是按**自然窗口**（日/周/月）设计的，`credit_window` 只有 `day`/`week`/`month` 三档。**这两者对不上**：
> - 若把 `credit_budget = 4000` + `credit_window = 'month'`，本地台账会在每月 1 日归零，但上游的钱包是在「购买日 + 31 天」到期——**每月初会多算额度，可能导致超额请求**
> - 正确做法：**不要用积分台账管 Trae 的积分**，改用 `credit_unit = 'requests'`（只限流、不记账）+ **依赖上游的 `ide_user_ent_usage` 接口定期同步真实余额**（见 §4.5 的上游校准）
>
> 这个发现修正了 §4.6 的初版结论：**Trae 国内站不应该进积分台账**。

**海外站用 `credit_unit = 'currency'`**：它的「Usage」本质就是美元，现有 `pricing` / `cost` 机制已经能算，不需要新机制。

**协议层（社区实测，来源 `wangqi233/trae2api` 的 `docs/PROTOCOL.md` + `autumnsentiment/Trae2api-cn`）**：

| 项 | 值 |
|---|---|
| 国内站 | `https://www.trae.cn`（`trae.com.cn` 301 到此）、IDE `https://www.trae.cn/ide/`、`https://work.trae.cn/` |
| 海外站 | `https://www.trae.ai` |
| 国内文档 | `https://docs.trae.cn/ide/what-is-trae` |
| 鉴权头 | `Authorization: Cloud-IDE-JWT <token>` |
| 本地凭据位置 | `%APPDATA%\TRAE SOLO CN\User\globalStorage\storage.json`，键 `iCubeAuthInfo://icube.cloudide` |
| 凭据加密 | `tc` 格式 = AES-128-CBC，key/iv = `SHA-512(SHA-512(RandomBytes) XOR salt64)`，**盐硬编码在客户端前端 JS 里** → 「拷走 storage.json 即可离线解密」成立。国际版部分版本是**明文 JSON** |
| 必备请求头 | `x-app-id`、`x-device-id`、`x-machine-id`、`x-ide-version`、`x-ide-version-code`、`x-device-brand`、`x-os-version`、`x-flow-traceparent` |
| **流式** | `llm_utils_chat` **无论 `stream:false` 与否都回 SSE**，解析一律按 SSE |
| **content 形态** | 所有 message 的 `content` 必须是数组（`[{type:'text',...}]`）；字符串 → 400 + `4001 cannot unmarshal string` |
| **tools 坑 1** | `tools[].function.parameters` **必须是 JSON 字符串**（传 object → 400） |
| **tools 坑 2** | 上游发 tool_calls 时 `finish_reason` 仍是 `"stop"`，**必须改写为 `"tool_calls"`**，否则客户端不执行工具 |
| **reasoning 字段** | 就是 `reasoning_content`（在 SSE `output` 事件里），映射到 `reasoning_effort` 三档 `light` / `high` / `extra_high` |
| **超长静默断流** | `query` 超约 50 万字符上游**静默断流**（不报错，直接结束流）——必须靠客户端侧的长度检查兜底 |
| 双区模型名 | **不通用**：CN / SG 各 47 个模型只有 3 个交集（`glm-5.2`、`minimax-m3`、`summary`）。**UI 展示名 ≠ config_name**（INTL 的 `Seed-2.1-Turbo` 实际 key 是 `Dola-Seed-2.0-Code`） |
| `ExchangeToken` 副作用 | **轮换整个令牌族**，会让同账号在其它客户端的登录静默失效 |

**域名角色对照（已核实 / 社区实测分开标注）**：

| 域名 | 角色 | 等级 |
|---|---|---|
| `www.trae.cn` | 国内站官网 | 已核实（HTTP 200） |
| `www.trae.ai` | 国际站官网 | 已核实 |
| `api.trae.cn` | CN **商业/成长面**：签到、额度查询、`terms/get`、`GetUserInfo` | 已核实（本次实测 POST 打通 200） |
| `api.trae.com.cn` | CN 账号/OAuth/pay 备用 | 社区实测（根路径 404，正常） |
| `trae-api-cn.mchost.guru` | CN **模型数据面**（`/api/ide/*`、`/api/agent/*`、`/api/remote/*`） | 社区实测 |
| `core-normal.trae.ai` / `coresg-normal.trae.ai` | INTL 模型数据面（美 / 新） | 已核实（前端 bundle 常量）+ 社区实测 |
| `ug-normal.trae.ai` | INTL 成长/权益（`terms/get` 在此返英文 ToS） | 已核实（本次实测） |
| `console.enterprise.trae.cn` | 企业版 OpenAPI base（`/openapi/v1/`） | 已核实（官方文档） |

**四个可切换的上游端点**（来源 `autumnsentiment/Trae2api-cn` README，★80 / 2026-10-23）：

| 端点 | `UPSTREAM_MODE` | 上游路径 | 说明 |
|---|---|---|---|
| Remote（默认） | `remote` | `POST {core}/api/remote/v1/chat_sessions` → `GET {core}/api/remote/v1/chat_sessions/{id}/events?reply_to_message_id={mid}` | 两步：建回合拿 `chat_session_id`+`message_id`，再拉 SSE |
| Solo | `solo` | `POST {code}/api/ide/v2/llm_raw_chat` | `raw_chat_function=solo_agent` |
| IDE Agent | `ide` | 同上 | `raw_chat_function=chat_v3` / `builder_v3` |
| Work | `work` | `POST {core}/api/remote/v1/chat_sessions`，固定 `agent_type=agent_id=solo_work_remote` | TraeWork 产品线 |

第三条路径（另两个项目）：`POST {core}/api/agent/v3/llm_utils_chat`，`function=solo_work_lite` + `config_name`。

> **基址分离陷阱**：`/api/remote/v1` 只在 Remote 通道用；IDE/Solo 路由 rooted 在同一主机根，混用会生成 `/api/remote/v1/api/ide/...` → 404/502。
> **重要限制**：个人账号的 `Cloud-IDE-JWT` 没有企业 app config 时，`/api/ide/v2/llm_raw_chat` 固定返回 `2001 failed to get app config: record not found`——**这不是请求体或模型名能修的错，必须企业版 app config / PAT**。所以 A3 第一版**只用 `remote` 通道**。

**额度查询接口（社区实测）**：

- `POST https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage`，body `{"require_usage":true,"req_source":0}` → `user_entitlement_pack_list[].entitlement_base_info.quota.credits_limit`（-1 = 无限）+ `usage.credits_amount`
- 单回合：`POST /api/v1/commercial/get_session_usage`，body `{"session_id":"<userMessageId>"}` → `credits_float`
- **CN 的通用积分与 Work 专属积分是两个钱包**（`req_source` 1/2）

错误码语义（**这张表是网关映射的依据**）：

| 码 | 含义 | 层级 | 网关应对 |
|---|---|---|---|
| `4001` | 模型名不在当前区域目录（官方文案是「请稍后重试」，**有误导性**） | 请求级 | 400，**不换号、不冷却** |
| `1005` | 套餐权益不足（模型在订阅档位外） | 模型级 | 400，**不换号、不冷却** |
| `4008` | 账号配额耗尽 | **账号级** | 429 + 冷却该平台 + 计入额度台账 |
| `4010` | **检测到风险账号，已自动登出** | 账号级 | 立即禁用平台 + 告警，**不重试** |
| `4015` | **帐户/IP 地址存在风险，请求已被阻止** | 环境级 | 长冷却 + 告警；**换号无效，继续换号会放大封禁** |
| `4017` | 设备 / IP 级风控 | 环境级 | 同上 |
| `4023` | 模型未知（BYOK 槽位未配 Key） | 模型级 | 400 |
| `3004` / `4007` | 限流 | 请求级 | 退避重试 |
| `4011` | 用量上限（**UA 不对时也会误报这个码**） | 混合 | **必须配合 UA 校验判定**，不能单独当限流 |
| `9074` | 签到接口风控 | — | 本方案不做签到 |

### 2.4 WorkBuddy（社区实测 + 官方文档，A 类）

> 来源：`yuan240324/workbuddy-openai-proxy`（★43，2026-09-27）的 `src/config.mjs` / `src/headers.mjs` / `src/upstream.mjs`，以及腾讯官方文档。

**站点预设（`src/config.mjs` 原文）**：

| 站点键 | apiBase | billingBase | User-Agent |
|---|---|---|---|
| `cn-cli` | `https://copilot.tencent.com` | `https://www.codebuddy.cn` | `CLI/2.63.2 CodeBuddy/2.63.2` |
| `intl-cli` | `https://www.codebuddy.ai` | `https://www.codebuddy.ai` | 同上 |
| `intl-work` | `https://www.workbuddy.ai` | `https://www.workbuddy.ai` | 同上 |

**端点**：

| 用途 | 方法与路径 |
|---|---|
| 对话 | `POST {apiBase}/v2/chat/completions`（**上游只接受流式，必须强制 `stream:true`**） |
| 模型目录 | `GET {apiBase}/console/enterprises/personal/models` → `data.models[]` / `data.agents[]` |
| 额度查询 | `POST {billingBase}/v2/billing/meter/get-user-resource` |

**模型目录响应字段**（决定了模型同步怎么落库）：
`id`、`name`、`credits`（积分倍率字符串，如 `x0.79 credits`；`x0.00` = 不扣积分）、`maxInputTokens`、`maxOutputTokens`、`supportsImages`、`supportsToolCall`、`supportsReasoning`。

**额度查询请求体**：
```json
{ "PageNumber": 1, "PageSize": 100, "ProductCode": "p_tcaca", "Status": [0, 3],
  "PackageEndTimeRangeBegin": "YYYY-MM-DD HH:mm:ss", "PackageEndTimeRangeEnd": "YYYY-MM-DD HH:mm:ss" }
```
响应：`data.Response.Data.Accounts[]`，字段 `CycleCapacitySize` / `CycleCapacityUsed` 等。

**身份头**：`X-User-Id` / `X-Enterprise-Id` / `X-Domain` / `X-Product: SaaS` / `X-Request-ID` / `X-Request-Trace-Id`。
缺哪个就发对应的否定头：`X-No-User-Id: 1` / `X-No-Enterprise-Id: 1` / `X-No-Department-Info: 1`。

**刷新头**：`X-Refresh-Token: <refreshToken>` + `X-Auth-Refresh-Source: workbuddy`（**只允许出现在刷新接口**）。

**请求体改写规则（三条都是硬性 400）**：
1. 国际版**第一条消息必须是 `system`**，否则 400 `first message is not system prompt`。
2. `role: "developer"` 必须改写为 `"system"`（上游 role 白名单没有 developer）。
3. `tool_choice` **只接受字符串**；对象形式会 400，需归一（`{type:'auto'}` → `"auto"`、`{type:'function',function:{name}}` → `"name"`、`none` → 删掉 `tools`）。

**客户端指纹黑名单（最重要的坑）**：上游对 system 消息做**精确匹配**，命中就回
`400 Illegal API invocation from an unapproved channel`。
实测确认被拦的原句是 `You are Claude Code, Anthropic's official CLI for Claude.`（整句），
拆开成 `You are Claude Code` 或 `Anthropic's official CLI for Claude` 都不拦，放 user 位置也不拦。
→ **网关必须在转发前剥离命中的那一句**（只剥这一句，system 其余内容原样保留）。

**其它实测事实**：
- 国际版约 17% 的 `ECONNRESET`，需要连接级重试（不重试业务错误）。
- 目录里的 `maxInputTokens` 会**偏大**（`glm-5.1` 目录 200000 / 真实 100000），需按上游报错学习真实上限。
- 额度不通用：国内版与国际版是两套独立账号、两份额度。

### 2.5 官方文档口径（合规章的依据）

WorkBuddy 官方文档 `.../Function-Description/Model` 明确写着：

> 配置参数（含 API Key）仅保存在本地 `workbuddy/models.json`，不上传云端。
> 使用时，WorkBuddy 仅作为通信链路，将您的输入直接转发至所配置的第三方模型……
> **自定义模型产生的全部费用由您向第三方支付。**

也就是说：**官方支持把网关当地「自定义模型」接进去**——这是**反方向**的集成（WorkBuddy 调网关），完全合规，而且我们的网关正好是 OpenAI 兼容的，天然支持。

而本方案的 A 类是**正方向**（网关调 WorkBuddy 的额度），官方**没有**提供 API，属于逆向。两者必须区分清楚。

### 2.6 🔴 上游封堵：A 类的可行性已被现实削弱（2026-10 最新事实）

**这是本文档最重要的一条风险，必须在动手前就知道。**

两个主力 WorkBuddy 反代项目在 2026-10 下旬先后收缩：

| 项目 | ★ | 状态 | 官方给出的原因（README 原文） |
|---|---|---|---|
| `linguo2625469/workbuddy2api-panel` | **4561** | **停止维护（2026-10-21）** | 「**上游已开始禁用反代**——WorkBuddy 对非官方客户端的识别与拦截正在收紧（通道校验、账号限制等），反代通道的稳定性与存活周期已不可控」；另有「非法转卖泛滥」 |
| `ardeyouxipianyi/workbuddy2api-hub` | **3064** | **低频维护（2026-10-23）** | 「**WorkBuddy 上游正在收紧对非官方客户端的识别与拦截**。2026-10-23 下午起，非官方客户端的识别与拦截明显收紧……上游可能随时进一步收紧，导致本项目不可用」 |

与此同时，Trae 侧的项目仍在**活跃更新**（`autumnsentiment/Trae2api-cn` ★80 / 2026-10-23、`shuishuipingan/trae2api-hub` ★19 / 2026-10-23、`XCool-603/trae2api` / 2026-10-27），且 Trae 项目仍在**增加新功能**（图片输入、Responses API、思考强度映射、多端点切换）。

**结论（会改变交付顺序）**：

1. **WorkBuddy 的 A 类（逆向）建议降级为「可选 / 延后」**，不作为第一阶段目标。理由不是技术上做不到，而是**上游正在主动封堵**，做完可能很快就失效，投入产出比差。
2. **Trae 的 A 类仍然值得做**，生态活跃、协议有完整实测记录（`wangqi233/trae2api` 的 `docs/PROTOCOL.md`）。
3. **Agnes / 魔搭（B 类）与 Key 快捷入口（C 类）优先级进一步提升**——它们是唯一「不会因为上游封堵而一夜失效」的部分。

> 这个结论直接决定了 §13 的交付顺序：S1（Key 入口）+ S2（预设）先行，Trae 适配器次之，WorkBuddy 适配器最后且标为实验性。

### 2.7 WorkBuddy 上游的风控对抗点（做 A 类必须知道的完整清单）

社区项目已经总结出一整套「让请求看起来像官方客户端」的对抗点。**这张表本身就是风险清单**——每一项都说明上游在主动识别非官方客户端：

| 维度 | 社区做法 | 若不做的后果 |
|---|---|---|
| **UA 三段式** | `WorkBuddy/<ver> <platform>/<ver> CLI/<ver>`；国际版 platform 必须是 `WorkBuddy AI` | 403 / `11140` |
| **`X-CodeBuddy-Request: 1`** | 所有 API 必带的「闸门头」 | 被识别为非官方客户端 |
| **`X-Domain` / `X-No-Enterprise-Id`** | 国际版固定声明 `www.workbuddy.ai` | 身份校验失败 |
| **会话头族完整性** | 缺 `X-Conversation-ID` 会 400 / `11128 conversation id missing` | 请求被拒 |
| **设备指纹** | `X-Machine-ID` / `X-Session-ID` 按 uid 稳定派生（不随机漂移，防跨账号关联）；`X-Device-Token` 来自客户端 | 跨账号关联 → 批量封禁 |
| **system prompt 指纹** | 客户端 system 模板句被上游**逐字精确匹配**后 400（`blocked by security policy` / `unapproved channel` / `illegal api invocation`） | 400，且换号无效 |
| **消息内容指纹脱敏** | 出站 body 黑名单字段清洗（客户端名 / 版本 / 路径） | 400 |
| **身份句改写** | `Claude Code` / `Codex` / `DeepSeek Harness` → `WorkBuddy` 文本级替换 | 400 |
| **并发限流** | 国际版 WAF 更紧，单账号在途并发需压低（默认 global 2 vs 国内 3） | 429 / 冷却 |
| **请求体体积** | 上游按**请求体积**（非 token 数）触发 400，约 34~40 万字符；报错里的 token 数字**不可信** | 长请求硬失败 |
| **`/v2/responses` 不存在** | 实测返回 404 `Route Not Found`，Responses 必须网关侧转换 | — |
| **企业号无成长体系** | 签到 / 成长任务 / 连登 / 旅行对企业号一律被拒 | — |
| **企业切换会话** | `/v2/report` 返回 200 但**静默丢弃** | 记账不准 |

**上游收紧的时间线**（已核实）：

- `2026-09-24`：桌面端凭据改为 `$wbEncrypted` 加密信封（AES-256-GCM + `WB-AAD`），密钥编译在原生模块里 → 「从本机客户端导入凭据」的成本大幅提高
- `2026-10-21`：`workbuddy2api-panel`（★4561）宣布停止维护，理由「上游已开始禁用反代」
- `2026-10-23`：`workbuddy2api-hub`（★3064）公告「下午起上游识别与拦截明显收紧」，转低频维护

> **对本方案的直接影响**：如果要做 WorkBuddy 的 A 类，**必须实现上表的前 7 项**，否则连基本可用都做不到。这不是「加个 header」的量级，而是一个持续对抗的维护负担——这正是把它排到最后、标为实验性的理由。
>
> 反过来，**本方案明确不实现**后几项中属于「主动规避」的部分（设备指纹伪造、身份句改写以伪装成官方客户端）。只做「如实携带用户自己的凭据与设备标识」，不做「伪造」。这条界线在 §10.2 有详细说明。

---

## 3. 总体架构

### 3.1 分层

```
                    ┌─────────────────────────────────────────┐
  客户端            │  /v1/chat/completions  /v1/messages     │  ← 不变
  (Claude Code…)    └──────────────────┬──────────────────────┘
                                       │
                    ┌──────────────────▼──────────────────────┐
                    │  RouteCache（不变）                      │
                    │  proxy → route → backend[] → platform    │
                    └──────────────────┬──────────────────────┘
                                       │
                    ┌──────────────────▼──────────────────────┐
                    │  BackendSelector（不变，天然轮转多账号）  │
                    └──────────────────┬──────────────────────┘
                                       │
              ┌────────────────────────┼────────────────────────┐
              │                        │                        │
     ┌────────▼────────┐    ┌──────────▼─────────┐   ┌──────────▼─────────┐
     │ OpenAI 协议路径  │    │ Anthropic 协议路径  │   │ ★ 私有协议路径     │
     │ （现有，不变）   │    │ （现有，不变）      │   │ PlatformAdapter    │
     └─────────────────┘    └────────────────────┘   │  Trae / WorkBuddy  │
                                                     └──────────┬─────────┘
                                                                │
                                          ┌─────────────────────┼─────────────────────┐
                                          │                     │                     │
                                  ┌───────▼───────┐   ┌─────────▼────────┐  ┌────────▼────────┐
                                  │ 凭据保险箱     │   │ 请求体改写        │  │ SSE 解析         │
                                  │ credentials   │   │ system/role/tc    │  │ 私有事件 → 统一   │
                                  └───────┬───────┘   └──────────────────┘  └─────────────────┘
                                          │
                                  ┌───────▼────────┐
                                  │ 刷新 / 额度同步 │  ← 后台任务
                                  └────────────────┘
```

### 3.2 三条路径的判定

现有 `build_forward_request` 按 `platform_type` 二分（`Anthropic` vs 其它）。改为**三分**：

```rust
match platform.adapter {
    PlatformAdapter::OpenAI    => /* 现有 openai::to_request + {base}/chat/completions */,
    PlatformAdapter::Anthropic => /* 现有 anthropic::to_request + {base}/v1/messages */,
    PlatformAdapter::Trae      => trae::to_request(...),      // 新
    PlatformAdapter::WorkBuddy => workbuddy::to_request(...), // 新
}
```

**`adapter` 与 `platform_type` 的关系**：`platform_type` 表达「上游的**响应**协议族」（决定响应怎么解析、是否跨协议转换）；`adapter` 表达「上游的**请求**构造方式」。两者正交，但默认值由 `platform_type` 推出（`OpenAI` → `OpenAI`、`Anthropic` → `Anthropic`），所以**现有 15 个预设零改动**。

> 为什么不用 trait：项目现有风格是枚举 + match（`PlatformType`、`ReasoningStyle`、`Protocol` 都是），没有 trait object。引入 trait 会带来对象安全、生命周期、Send 约束的额外复杂度，收益不大。四个变体、两处 match，枚举足够。

---

## 4. 数据模型与迁移 V28

### 4.1 `platforms` 新增 `adapter` 列

```sql
ALTER TABLE platforms ADD COLUMN adapter TEXT;  -- NULL = 由 type 推出（兼容旧行）
```

- `NULL` / `"OpenAI"` / `"Anthropic"` / `"Trae"` / `"WorkBuddy"`
- 读时归一：`adapter ?? platform_type 推出的默认值`
- 走 `add_column_if_missing`（现有迁移都这么写，因为测试会回卷 `_schema_version` 重跑）

### 4.2 `platform_credentials`（新表）

**为什么不复用 `platforms.api_key`**：`api_key` 是单个字符串，而 A 类凭据是结构体（accessToken + refreshToken + uid + enterpriseId + domain + 到期时间 + 状态），并且有独立的生命周期（刷新、失效、多账号）。塞进一个字符串再用 JSON 解析，会让「Key 掩码」「测试连接」「配置导出」全部要特判。

```sql
CREATE TABLE IF NOT EXISTS platform_credentials (
    platform_id     TEXT PRIMARY KEY REFERENCES platforms(id) ON DELETE CASCADE,
    -- AES-256-GCM 加密后的 JSON blob，密钥来自 config.toml 的 encrypt_key
    -- 明文结构：{access_token, refresh_token, uid, enterprise_id, domain, expires_at}
    secret_blob     TEXT NOT NULL,
    -- 凭据形态：'api_key' | 'oauth'。api_key 形态时 secret_blob 里只有 access_token
    kind            TEXT NOT NULL DEFAULT 'api_key',
    -- 最近一次刷新成功的时刻；NULL = 从未刷新
    refreshed_at    TEXT,
    -- 刷新失败次数与最近错误（连续失败到阈值就把平台置 Disabled + 告警）
    failure_count   INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    -- 是否允许网关自行刷新。默认 0：Trae 的 ExchangeToken 会轮换令牌族，
    -- 自刷会让同账号在其它客户端的登录静默失效（见 §2.3）
    allow_self_refresh INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
```

**加密方式**：复用现有 `aes-gcm = "0.10"` 依赖（`Cargo.toml:45` 已引入）与 `config.security.encrypt_key`。
`secret_blob` 的序列化格式：`base64(nonce[12] || ciphertext || tag[16])`。
密钥派生：`SHA256(encrypt_key)` → 32 字节 AES-256 key（与现有 `platforms.api_key` 的加密口径保持一致，实现时对齐现有函数，不要新造一套）。

> **注意**：`encrypt_key` 默认值是 `ai-gateway-default-key-change-me`。若用户从未改过，加密等同于明文。设计上不阻塞，但要在 UI 上对「仍在使用默认 encrypt_key」给出一次性警告（这是既有问题，不是本需求引入的）。

### 4.3 多账号 = 同 `catalog_key` 多行（复用 V25）

`catalog_key = SHA256(type_key | normalize_base_url)`（`src/catalog.rs`）。

A 类上游的 base_url 是**固定的**（Trae CN `https://api.trae.com.cn`、Trae SG `https://coresg-normal.trae.ai`、WorkBuddy CN `https://copilot.tencent.com`、WorkBuddy intl `https://www.workbuddy.ai`），所以：

- 同一区、同一产品的多个账号 → 自动共享一份模型目录与价格 ✅
- 不同区 → base_url 不同 → 不同 catalog_key → 目录独立 ✅（这正是我们要的，因为双区模型名不通用）

**陷阱**：Agnes 的国内站与海外站是**两个不同的 base_url**（`https://api.agnes-ai.cn/v1` / `https://apihub.agnes-ai.com/v1`），`catalog_key` 天然不同，目录天然独立——**不要**为了「统一」而把两行的 base_url 改成同一个值，否则会把两套账号的模型目录合并成一份，价格互相覆盖。

反过来也要注意：**不要为了「区分账号」而给同一个 base_url 造不同的 catalog_key**。同一 base_url 的多个账号（比如两个 WorkBuddy 国内号）共享目录才是正确的——这正是 V25 的收益。

`type_key` 的取值需要扩展：现有实现按 `PlatformType` 生成。A 类引入 `adapter` 后，`type_key` 应改为 `format!("{:?}:{:?}", platform_type, adapter)`，让「同为 OpenAI 协议但 adapter 不同」的两个平台不会误共享目录。

### 4.4 `models` 新增 `credit_multiplier` 列

WorkBuddy 的模型目录直接给出**积分倍率**（`credits: "x0.79 credits"`），这比「每次请求固定 1 分」精确得多。

```sql
ALTER TABLE models ADD COLUMN credit_multiplier REAL;  -- NULL = 1.0
```

积分口径扩展为：`本次消耗 = models.credit_price × (models.credit_multiplier ?? 1.0)`。
现有 `credit_price` 语义不变（NULL = 0 = 未定价），乘数只是额外因子，**不改变已有数据的行为**。

> 若某模型 `credits: "x0.00"`，则乘数 0 → 消耗 0，但仍记一条 `credits = 0` 的统计（与现有 `CreditReserve::Free` 语义一致）。

### 4.5 `platform_credit_state` 复用 + 上游校准

现有台账是「本地扣减」模型（`try_reserve` / `release` / `adjust` / `reset`）。A 类上游有**权威额度接口**，两者会漂移。

**策略：本地为准，上游校准。**

- 热路径仍然走本地 `try_reserve`（零网络、零 DB 热点）——不能用上游接口，那会让每个请求多一次往返。
- 后台任务（每 30 分钟 + 手动触发）调用上游额度接口，把权威值与本地 `spent` 比对：
  - 偏差 ≤ 1 分：不动
  - 偏差 > 1 分：`adjust(delta)` 并写一条 `calibrate` 流水，`note` 记「上游 X / 本地 Y」
- 上游报「已耗尽」而本地还有余额：直接把 `spent` 置为 `budget`（保守），写 `calibrate` 流水。

新增流水类型 `calibrate`（现有是 `charge` / `release` / `adjust` / `reset`）。

### 4.6 计量单位必须在平台行上声明（否则会算错）

**这是本方案最容易出错的地方**，因为四个上游的「额度」语义完全不同：

| 上游 | 额度单位 | 窗口 | 应该用哪套 |
|---|---|---|---|
| Trae 国内站 | **积分**（4000/月） | 自然月 | `credit_budget` + `credit_window = 'month'` |
| Trae 海外站 | **Usage（美元）** | 月 | **不用积分台账**，走现有 `pricing` / `cost` |
| WorkBuddy | **积分**（credits，含倍率） | 自然月 | `credit_budget` + `credit_multiplier` |
| Agnes 国内站 | **Credits**（1,000 Credits = ¥1） | 订阅周期 | `credit_budget` + `credit_window = 'month'` |
| Agnes 海外站 | **请求次数**（Token Plan） | 5 小时 / 周 | 不适合积分台账（窗口不是自然月），**走限流而不是额度** |
| 魔搭 | 每日调用次数 | 自然日 | 不适合积分台账，**走限流** |

**结论**：`platforms` 增加一列 `credit_unit`，取值 `credits` / `currency` / `requests` / `none`（默认 `none`）。

- `credits`：走积分台账（`try_reserve` / `release`），**窗口必须是「自然窗口」**
- `currency`：走现有 `cost` 计价，不进积分台账
- `requests`：不建额度台账，只用 RPM 限流 + 上游余额同步
- `none`：不计量

**最终判定表（关键：Trae 与 Agnes 海外都不能用 `credits`）**：

| 上游 | 上游单位 | 上游窗口 | `credit_unit` | 为什么 |
|---|---|---|---|---|
| WorkBuddy | credits（含倍率） | 自然月 | `credits` | 唯一真正适配自然月窗口的 |
| Trae 国内站 | 积分 | **31 天钱包**（非自然月） | **`requests`** | 钱包到期日 ≠ 每月 1 日，台账会在月初多算额度 |
| Trae 海外站 | Usage（美元） | 月 | **`currency`** | 本质是钱，走 `cost` |
| Agnes 国内站 | Credits（1000 = ¥1） | 订阅周期 | **`currency`** | 它是钱的另一种写法（¥1 = 1000 Credits），走 `cost` 更准 |
| Agnes 海外站 | 请求次数 | 5 小时 / 周 | **`requests`** | 窗口是滚动 5 小时，不是自然窗口 |
| 魔搭 | 每日调用次数 | 自然日 | **`requests`** | 同上，且无成本信息 |

> **这是一个重要的自我修正**：初版设计想让 Agnes 国内站走 `credits`，但它 1,000 Credits = ¥1，本质是货币；而 Trae 国内站的积分是 31 天钱包，与 `credit_window` 的自然月语义冲突。
> **结论：真正能用 `credits` 台账的只有 WorkBuddy 一家。** 其余上游用 `currency`（走 cost）或 `requests`（走限流 + 上游余额同步）。
> 这个结论让 §4.5 的「上游校准」变得更重要——因为大部分上游不再由本地台账管额度，而要靠定期同步上游真实余额。

```sql
ALTER TABLE platforms ADD COLUMN credit_unit TEXT;  -- NULL = 'none'
```

---

## 5. 后端：上游适配层

### 5.1 新增文件

```
src/proxy/upstream/
├── mod.rs          # PlatformAdapter 枚举 + 统一入口
├── trae.rs         # Trae 私有协议
├── workbuddy.rs    # WorkBuddy / CodeBuddy 私有协议
└── error_map.rs    # 上游错误码 → 网关语义
```

### 5.2 `PlatformAdapter`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlatformAdapter {
    OpenAI,
    Anthropic,
    Trae,
    WorkBuddy,
}

impl PlatformAdapter {
    /// 旧行没有 adapter 列，由 platform_type 推出默认值
    pub fn resolve(stored: Option<&str>, platform_type: &PlatformType) -> Self { … }

    /// 是否走私有协议（决定是否需要凭据保险箱、是否需要请求体改写）
    pub fn is_proprietary(&self) -> bool {
        matches!(self, Self::Trae | Self::WorkBuddy)
    }
}
```

### 5.3 统一入口签名

```rust
pub struct ForwardPlan {
    pub url: String,
    pub body: serde_json::Value,
    pub headers: Vec<(String, String)>,  // 不含 Authorization，由调用方统一注入
    /// 是否强制流式（WorkBuddy 上游只接受流式）
    pub force_stream: bool,
}

pub fn build(
    adapter: PlatformAdapter,
    platform: &Platform,
    backend: &Backend,
    unified: &UnifiedRequest,
    creds: Option<&Credential>,
    default_max_tokens: Option<u32>,
) -> AppResult<ForwardPlan>;
```

`src/proxy/handler.rs` 里 `build_forward_request` 改为调用它，并在 `req_builder` 之后追加 `plan.headers`。

### 5.4 WorkBuddy 适配器要点

```rust
// 1. 强制流式（上游只接受 stream=true）
body["stream"] = json!(true);

// 2. role 归一：developer → system
for m in messages { if m.role == "developer" { m.role = "system" } }

// 3. 首条必须是 system（国际版硬性要求）
if messages[0].role != "system" { messages.insert(0, system(default_prompt)) }

// 4. 剥离客户端指纹（只剥命中的那一句，其余原样保留）
//    命中：/You are Claude Code,\s*Anthropic's official CLI for Claude\.?/i
//    剥离后为空 → 用 fallback prompt 兜底

// 5. tool_choice 归一为字符串
//    {type:"auto"} → "auto"；{type:"function",function:{name}} → name；none → 删 tools

// 6. 路径：{apiBase}/v2/chat/completions
```

**响应**：一律按 SSE 解析（即使 `stream:false`）。解析出的 `choices[0].delta` 需要转换成统一格式，走现有 `stream_convert` 的跨协议路径（客户端若是 Anthropic 协议，复用现有 OpenAI → Anthropic 转换器）。

**请求头**（完整列表见 §2.4）：`X-User-Id` / `X-Enterprise-Id` / `X-Domain` / `X-Product: SaaS` / `X-Request-ID`（16 字节 hex）/ `X-Request-Trace-Id`（UUID），缺哪项发对应 `X-No-*` 否定头。

### 5.5 Trae 适配器要点

```rust
// 1. 鉴权头：Authorization: Cloud-IDE-JWT <access_token>
// 2. 必备头：x-app-id / x-device-id / x-machine-id / x-ide-version /
//            x-ide-version-code / x-device-brand / x-os-version / x-flow-traceparent
// 3. 路径：POST {base}/api/v1/llm_utils_chat（具体路径以社区实测为准，落地时抓包复核）
// 4. config_name = backend.model_id（必须在 get_detail_param 列表内，否则 4001）
// 5. function = "solo_work_lite"（轻排队）或 "chat_v3"（重排队，仅当模型只在 chat_v3 目录）
// 6. 所有 message 的 content 必须是数组：字符串 → [{type:"text", text}]
// 7. 无论 stream 与否都回 SSE → 一律按 SSE 解析
```

> **`x-device-id` / `x-machine-id` 的来源**：来自本地客户端 `storage.json`。网关**不伪造**这些值——用户在网关里填的是「从自己客户端导出的凭据」，设备标识随凭据一起提供。这既是技术必需，也是合规边界（不生成虚假设备指纹）。

### 5.6 错误码映射（`error_map.rs`）

```rust
pub enum UpstreamFailure {
    /// 模型不在目录 / 权益不足 / 参数非法：请求级，不换号、不冷却
    ClientError { status: u16, message: String },
    /// 账号配额耗尽：换号 + 冷却 + 记账
    QuotaExhausted { retry_after_secs: u64 },
    /// 设备 / IP 风控：换号通常无效，长冷却 + 告警
    RiskControl { cooldown_secs: u64 },
    /// 凭据失效：触发刷新；刷新失败则禁用平台
    Unauthorized,
    /// 连接级抖动（ECONNRESET 等）：可重试
    Transport,
}
```

映射表：

| 上游 | 码 / 特征 | 映射 |
|---|---|---|
| **Agnes** | **`402`** | **`QuotaExhausted { 60 }`** —— Agnes 额度耗尽返回 402 而非 429，必须单独映射，否则不换后端、不进积分台账 |
| **Agnes** | `429` | 限流（免费用户文本 RPM = 10）→ 按限流重试 |
| Trae | `4001` | `ClientError { 400 }` |
| Trae | `1005` | `ClientError { 400 }`（**不换号**） |
| Trae | `4008` | `QuotaExhausted { 60 }` |
| Trae | `4017` | `RiskControl { 3600 }` |
| Trae | `4023` | `ClientError { 400 }` |
| WorkBuddy | `402` | `QuotaExhausted`，硬冷却到次日 04:00 |
| WorkBuddy | `429` + `code 14018` | `QuotaExhausted`（**必须先于通用 429 判定**，否则误冷却 12h） |
| WorkBuddy | `429` + `6004` | 模型级限流 → 按上游重置时间冷却 |
| WorkBuddy | `429` + `6021` / `queue.full` | 渠道容量满载 → `503 upstream_busy`，**不罚号** |
| WorkBuddy | `400 Illegal API invocation from an unapproved channel` | `ClientError { 400 }`（指纹剥离失败，记 warn） |
| WorkBuddy | `400 first message is not system prompt` | `ClientError { 400 }`（改写逻辑有 bug，记 error） |
| WorkBuddy | `403` / `code 11140`（内容审核） | `ClientError { 403 }`，**不降级** |
| WorkBuddy | `11128` vs `11140`（都是 `request illegal`） | **必须 code + 文案双条件判定**，只看文案会把通道校验失败误判成封号并禁用健康账号 |
| 通用 | `401` / `403` token 失效 | `Unauthorized` |
| 通用 | `ECONNRESET` / `socket hang up` | `Transport`（重试，最多 3 次） |

**关键**：`ClientError` 必须**不触发换号**。现有重试逻辑是按 429/5xx/timeout 匹配的，需要确认 `ClientError { 400 }` 不会进入重试循环——否则一个「模型不在目录」的错误会被所有账号轮流撞一遍，白等 N × 超时。

**另一个关键**：`402` 在现有 `AppError` 里没有对应变体。要新增或复用 `TooManyRequests`，但**HTTP 状态码必须回 429 给客户端**（OpenAI 生态的客户端只认 429 是「限流/额度」），而不是把 402 原样透传——否则 Claude Code / Cursor 这类客户端会当成「未知错误」不做退避。

### 5.7 上下文压缩（A 类必需）

WorkBuddy 上游的 `maxInputTokens` 会偏大（`glm-5.1` 目录 200000 / 真实 100000），超限报错也不总是带 token 数字。社区项目的解法是「相对收缩 + 重试」：

1. 本地估算 messages + tools 的 token 数（**工具定义必须计入**，agent 客户端会带一大坨 MCP 工具定义，实测能占 90 万 token）
2. 预压缩到 `上限 × 0.95 − 工具定义`
3. 上游报「太长」→ 按 0.75（首次）/ 0.5（后续）收缩 messages，最多重试 3 次
4. 上游亲口报的真实 token 数用于校准本地估算器

**本方案取舍**：第一版**只做 3，不做 1/2**。理由：
- 本地 token 估算对中文长文本会低估近 2 倍（社区实测：真实 1,193,121 / 本地估 620,249），预压缩做不准
- 现有网关已经有 `request_timeout_secs = 600` / `stream_timeout_secs`，撞一次 400 再重试的代价可接受
- 上下文压缩是一个独立的大需求（要处理中文感知、工具定义、多轮裁剪策略），塞进本需求会失控

→ 第一版：**撞到「太长」就按 0.5 收缩 messages 重试一次**，仍失败则把上游原始错误如实透传。同时记 warn 日志，为后续的完整压缩方案收集真实数据。

---

## 6. 后端：凭据保险箱

### 6.1 生命周期

```
用户在 UI 粘贴凭据
      │
      ▼
POST /api/platforms/{id}/credentials
      │  校验形态（Trae: 至少 access_token + expires_at；
      │           WorkBuddy: access_token + refresh_token）
      ▼
AES-256-GCM 加密 → platform_credentials.secret_blob
      │
      ▼
请求热路径：读缓存 → 解密 → 构造鉴权头
      │
      ├─ 401/403 → 触发刷新（若 allow_self_refresh）
      │              ├─ 成功 → 更新 blob + refreshed_at，重放一次请求
      │              └─ 失败 → failure_count += 1
      │                        ≥ 3 → platform.status = Disabled + 告警
      ▼
后台任务（每 10 分钟）：
  - 扫描 expires_at 在 15 分钟内的凭据 → 刷新
  - 扫描 status = Disabled 且 failure_count < 3 的 → 重试一次
```

### 6.2 热路径不能读 DB

现有架构是「零 DB 热路径」（`RouteCache` 预 join 一切）。凭据解密要放在哪？

**方案**：`RouteCache` 的 `ResolvedPlatform` 里带上**已解密**的凭据。理由：
- 解密是纯 CPU（AES-GCM 微秒级），但每次请求都解一遍没必要
- 凭据变化是低频事件（刷新 / 用户修改），刷新后 `cache.refresh()` 即可

**代价**：明文凭据驻留内存。这与现有 `platforms.api_key` 的现状一致（`Platform.api_key` 就是明文常驻内存），不引入新的安全等级。要在代码注释里写明这个取舍。

### 6.3 `allow_self_refresh` 默认 0

Trae 的 `ExchangeToken` 会**轮换整个令牌族**——网关一刷，同账号在 Trae 客户端的登录就静默失效了。社区项目的默认也是 `off`。

- 默认 `0`：网关只用已导入的令牌，过期了就报 `Unauthorized` + 告警，让用户重新从客户端导入
- 用户显式打开（`1`）：网关自刷，但 UI 上要有明确警告「开启后，同一账号在官方客户端的登录会失效」

WorkBuddy 的刷新（`X-Refresh-Token`）**没有**这个副作用，所以 `kind = 'oauth'` 且 `adapter = WorkBuddy` 时默认可以给 `1`。

### 6.4 凭据不从客户端本地文件读取

**明确不做**「自动扫描 `%APPDATA%\Trae...\storage.json`」。理由：

1. 那是**读用户本机的其它应用数据**，属于越界（社区项目这么做，但那是「个人自用工具」，本项目是「网关」，可能被多人使用）
2. Trae 的凭据是 `tc` / AES-128-CBC 加密的，解密需要实现其专有算法，无文档、易变
3. 用户手动粘贴 `access_token` 完全可行，且边界清晰

→ UI 给一个「如何获取凭据」的说明面板（含客户端路径提示），但不代读。

---

## 7. 后端：Key 申请快捷入口

### 7.1 设计

一个**后端注册表 + 前端渲染**，而不是硬编码在前端。理由：加新平台时只改后端，且能被 `GET /api/platforms/presets` 复用。

```rust
// src/models/key_link.rs（新）
pub struct KeyLink {
    /// 与 PlatformPreset.name 或 base_url 匹配
    pub preset: Option<String>,
    pub base_url_prefix: Option<String>,
    /// 展示名（中英由前端 i18n 覆盖，这里只放品牌专有名词）
    pub brand: String,
    /// Key 申请页
    pub apply_url: String,
    /// 控制台 / 用量页
    pub console_url: Option<String>,
    /// 文档
    pub docs_url: Option<String>,
    /// 该平台是否 OpenAI 兼容（决定「用此预设」时 base_url 填什么）
    pub openai_compatible: bool,
    /// 实测可达性（'ok' | 'redirect' | 'auth-required' | 'unverified'）
    pub verified: String,
    /// 备注（一句话，如「国内站 / 海外站是两套账号」）
    pub note: Option<String>,
}
```

### 7.2 初始注册表（全部实测过状态码，2026-10-27）

| 品牌 | Key 申请页 | OpenAI 兼容 base_url | Key 页 | base_url 探活 |
|---|---|---|---|---|
| **魔搭 ModelScope** | `https://www.modelscope.cn/my/settings/token` | `https://api-inference.modelscope.cn/v1` | 200 | **200（免鉴权，37 个模型）** |
| **Agnes（海外）** | `https://platform.agnes-ai.com/settings/apiKeys` | `https://apihub.agnes-ai.com/v1` | 200 | 401（需鉴权） |
| **Agnes（国内）** | `https://platform.agnes-ai.cn/settings/apiKeys` | `https://api.agnes-ai.cn/v1` | 200 | 401（需鉴权） |
| OpenAI | `https://platform.openai.com/api-keys` | `https://api.openai.com/v1` | **403（反爬，未实测到 200）** | 401 |
| Anthropic | `https://console.anthropic.com/settings/keys` | 原生 Messages 协议（非 OpenAI 兼容） | 200 | 401 |
| DeepSeek | `https://platform.deepseek.com/api_keys` | `https://api.deepseek.com/v1` | 200 | 401 |
| 智谱 BigModel | `https://bigmodel.cn/usercenter/proj-mgmt/apikeys` | `https://open.bigmodel.cn/api/paas/v4` | 200 | 401 |
| Moonshot / Kimi | `https://platform.moonshot.cn/console/api-keys` | `https://api.moonshot.cn/v1` | 200 | 401 |
| Moonshot（海外） | `https://platform.moonshot.ai/console/api-keys` | `https://api.moonshot.ai/v1` | 200 | 401 |
| SiliconFlow | `https://cloud.siliconflow.cn/account/ak` | `https://api.siliconflow.cn/v1` | 200 | 401 |
| SiliconFlow（海外） | `https://cloud.siliconflow.com/account/ak` | `https://api.siliconflow.com/v1` | 200 | 401 |
| OpenRouter | `https://openrouter.ai/keys` | `https://openrouter.ai/api/v1` | 200 | **200（免鉴权）** |
| 阿里百炼 DashScope | `https://bailian.console.aliyun.com/?apiKey=1` | `https://dashscope.aliyuncs.com/compatible-mode/v1` | 200 | 401 |
| 火山方舟 Ark | `https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey` | `https://ark.cn-beijing.volces.com/api/v3` | 200 | 401 |
| Groq | `https://console.groq.com/keys` | `https://api.groq.com/openai/v1` | 200 | 401 |
| Together AI | `https://api.together.xyz/settings/api-keys` | `https://api.together.xyz/v1` | 200 | 401 |
| 腾讯云 LKEAP | `https://console.cloud.tencent.com/lkeap/api-key` | `https://api.lkeap.cloud.tencent.com/v1` | 200 | 401 |
| Trae（国内） | `https://www.trae.cn` | 私有协议 | 200 | — |
| Trae（海外） | `https://www.trae.ai` | 私有协议 | 200 | — |
| WorkBuddy（国内） | `https://www.workbuddy.cn` | 私有协议 | 200 | — |
| WorkBuddy（海外） | `https://www.workbuddy.ai` | 私有协议 | 200 | — |

**关于 `base_url 探活` 那一列**：401 是**故意不带 Key 请求 `/v1/models`** 的结果——401 恰好证明「端点存在且要鉴权」，**比 200 更能说明 base_url 正确**（200 也可能是个欢迎页）。只有 ModelScope 和 OpenRouter 是免鉴权直接 200 并返回真实模型列表的。

**`verified` 字段必须诚实**：OpenAI 的 Key 页只拿到 403（Cloudflare 拦非浏览器 UA，`curl` 与 `Invoke-WebRequest` 一致），URL 本身肯定是对的，但**不能标「已验证」**。这类条目在 UI 上显示「未实测」角标。这也是为什么这张表要放在**后端**：改一个 URL 不用重新构建前端。

### 7.3 ModelScope 的 URL 陷阱（必须写进代码注释）

`https://www.modelscope.cn/my/myaccesstoken` 是历史地址，前端路由表里已改成 `redirect: "/my/settings/token"`。**两个都能打开**，但预设里应写新地址。

> ⚠️ **验证方法上的坑**：ModelScope 是 umi SPA，**任意 `/my/xxx` 都返回 200**，所以「状态码 200」完全不能证明路径有效。上表的结论来自下载 `umi.js` 后 grep 路由表，不是靠状态码猜的。
> 对比：Agnes 控制台是 Next.js，**未知路径返回真实 404**（`/apikey`、`/api-keys`、`/keys` 全部 404），所以可以反向确认 `/settings/apiKeys` 有效。
> 两种相反的行为说明：**外链注册表的每一个 URL 都要用「页面里的路由表」或「真实的 404 语义」来验证，不能只看状态码**。

### 7.4 API

```
GET /api/platforms/key-links
→ [{ brand, apply_url, console_url, docs_url, openai_compatible, verified, note, preset }]
```

只读，不需要鉴权之外的额外限制（不含敏感信息）。

---

## 8. API 变更汇总

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/platforms/presets` | 返回增加 `adapter`、`key_link` 字段 |
| GET | `/api/platforms/key-links` | **新**：Key 申请入口注册表 |
| POST | `/api/platforms` | 请求体新增 `adapter`（可选，默认由 `type` 推出）、`credit_unit`（可选，默认 `none`） |
| PUT | `/api/platforms/{id}` | 同上 |
| PUT | `/api/platforms/{id}/credentials` | **新**：写入 / 更新上游凭据（不返回明文） |
| GET | `/api/platforms/{id}/credentials` | **新**：返回凭据**元信息**（kind / expires_at / failure_count / allow_self_refresh），**永不返回 secret** |
| DELETE | `/api/platforms/{id}/credentials` | **新**：清除凭据 |
| POST | `/api/platforms/{id}/credentials/refresh` | **新**：手动触发刷新 |
| POST | `/api/platforms/{id}/quota-sync` | **新**：手动触发上游额度同步与校准 |
| GET | `/api/platforms/{id}/remote-models` | 适配 `WorkBuddy`：解析 `data.models` + `data.agents`，落 `credit_multiplier` |
| GET | `/api/platforms/credits` | 返回增加 `upstream_remaining`（最近一次同步的权威值，可为 null）、`credit_unit` |

---

## 9. 前端

### 9.1 平台创建 / 编辑弹窗（`components/ProviderCreateModal.tsx`）

新增三块：

1. **「上游类型」Select**（放在「平台类型」旁边）
   - 选项：`OpenAI 兼容` / `Anthropic` / `Trae` / `WorkBuddy`
   - 选中 `Trae` / `WorkBuddy` 时：
     - 「API Key」字段切换为「凭据」面板（见 9.2）
     - Base URL 变成只读下拉（Trae: CN / SG 两个固定域名；WorkBuddy: 国内 `copilot.tencent.com` / 国际 `www.workbuddy.ai`），因为私有协议的 host 不能随便填
     - 显示「该上游无官方 API，接入方式为逆向客户端协议，请自行确认合规」的警示条
     - WorkBuddy 额外加「**实验性**」角标 + 「该上游正在收紧非官方客户端访问，随时可能失效」
2. **「额度单位」Select**（见 §4.6）
   - 选项：`不计费`（none）/ `积分`（credits）/ `金额`（currency）/ `请求次数`（requests）
   - 默认值由预设给出：Agnes 国内 → 积分；Agnes 海外 → 请求次数；魔搭 → 请求次数；Trae 国内 → 积分；Trae 海外 → 金额；WorkBuddy → 积分
   - 选 `积分` 时才显示「积分预算」+「重置窗口」两个字段（现有字段，逻辑不变）
   - 选 `金额` 时提示「走模型价格计价，无需积分预算」
   - 选 `请求次数` 时提示「不做额度台账，仅按 RPM 限流」
3. **「申请 API Key」按钮**（放在 Base URL 字段的 label 右侧）
   - 数据来自 `GET /api/platforms/key-links`
   - 按 `preset` 或 `base_url_prefix` 匹配当前选择
   - `verified != 'ok'` 时按钮上加「未实测」角标（用 `Badge`，颜色 `gray`）
   - `target="_blank" rel="noopener noreferrer"`

### 9.2 凭据面板

```
┌─ 上游凭据 ─────────────────────────────────────┐
│ 凭据状态：有效 · 到期 2026-11-03 14:22         │
│ [重新导入凭据]  [手动刷新]                      │
│                                                 │
│ access_token  [••••••••••••••]  [粘贴]          │
│ refresh_token [••••••••••••••]  [粘贴]          │
│ uid / enterprise_id（可选）                     │
│                                                 │
│ ☐ 允许网关自行刷新（开启后同一账号在官方客户端   │
│   的登录会失效）                                │
│                                                 │
│ ▸ 如何获取凭据                                  │
│   1. 打开 Trae 客户端并登录                     │
│   2. 凭据位于 %APPDATA%\TRAE SOLO CN\User\      │
│      globalStorage\storage.json                 │
│   3. …                                          │
└─────────────────────────────────────────────────┘
```

**注意 a11y**：所有输入用 `components/Field.tsx`（它用 `useId()` 绑 label），不要手写 `<div className="console-field"><label>`。警示条要有 `role="note"`，图标按钮要有 `aria-label`。

### 9.3 平台列表 / 详情

- 平台列表行：`credit_unit = credits` 的平台显示「额度（上游）」列，值来自 `GET /api/platforms/credits` 的 `upstream_remaining`；为 null 时显示 `—`（**不显示 0**）
- 平台列表行：`credit_unit = currency` 的平台显示成本列（现有逻辑）；`requests` / `none` 不显示额度列
- 平台详情：增加「额度同步」卡片（最近同步时间、上游值、本地值、偏差、手动同步按钮）；`currency` / `requests` / `none` 时整张卡片隐藏（不是显示 0）

### 9.4 新增平台预设（`src/presets.ts`）

在现有 15 个预设后追加：

| 预设名 | `platform_type` | `adapter` | `base_url` | `credit_unit` |
|---|---|---|---|---|
| `Agnes AI（海外）` | OpenAI | OpenAI | `https://apihub.agnes-ai.com/v1` | requests |
| `Agnes AI（国内）` | OpenAI | OpenAI | `https://api.agnes-ai.cn/v1` | currency |
| `ModelScope 魔搭` | OpenAI | OpenAI | `https://api-inference.modelscope.cn/v1` | requests |
| `Trae（国内）` | OpenAI | Trae | `https://trae-api-cn.mchost.guru` | requests |
| `Trae（海外）` | OpenAI | Trae | `https://core-normal.traeapi.us` | currency |
| `WorkBuddy（国内）` | OpenAI | WorkBuddy | `https://copilot.tencent.com` | credits |
| `WorkBuddy（海外）` | OpenAI | WorkBuddy | `https://www.workbuddy.ai` | credits |

> 预设的 `adapter` / `credit_unit` 只是**默认值**，用户创建后仍可在表单里改。这样「预设猜错了」不会成为死路。
>
> Trae / WorkBuddy 的 base_url 用社区实测的**模型数据面** host（`trae-api-cn.mchost.guru`、`core-normal.traeapi.us`、`copilot.tencent.com`、`www.workbuddy.ai`）。**这些 host 的根路径实测返回 404，这是正常的**（它们只接受特定 API 路径，不是网页站）——`api.trae.com.cn`、`coresg-normal.trae.ai`、`trae-api-cn.mchost.guru` 的 `/`、`/v1`、`/api` 全部 404。
> 所以「测试连接」按钮对这类平台**不能用「打 base_url 根路径」的方式探活**，必须用适配器构造真实请求（或直接跳过探活、只在首次真实请求时报错）。

### 9.4 i18n

所有新文案进 `frontend/src/i18n.ts`（zh + en）。中文模式只保留专有名词与单位：
`AI Gateway`、`Key`、`Token`、`API Key`、`Base URL`、`Trae`、`WorkBuddy`、`Agnes`、`ModelScope`、`credits`。
新增键（示例，落地时按实际需要补全）：
`adapter`、`adapterOpenai`、`adapterAnthropic`、`adapterTrae`、`adapterWorkbuddy`、`proprietaryWarning`、`applyApiKey`、`keyLinkUnverified`、`credentialStatus`、`credentialExpiresAt`、`reimportCredential`、`refreshNow`、`allowSelfRefresh`、`allowSelfRefreshWarning`、`howToGetCredential`、`quotaSync`、`quotaUpstream`、`quotaLocal`、`quotaDrift`、`syncNow`。

---

## 10. 合规与风险（必读）

### 10.1 三条路线的合规等级（已核实 ToS 原文）

| 路线 | 合规状态 | 依据 |
|---|---|---|
| **B：Agnes / 魔搭 API Key** | ✅ **完全合规** | 官方提供 API、官方文档教怎么用、按量付费 |
| **C：Key 申请页快捷入口** | ✅ **完全合规** | 只是外链 |
| **A2：Trae CLI + ACP / WorkBuddy 自定义模型** | ✅ **合规** | Trae：`docs.trae.cn/cli_agent-client-protocol` 官方文档支持；WorkBuddy：官方「自定义模型」功能 |
| **A3：逆向反代** | 🔴 **明确违约** | Trae 用户协议 5.2 / 5.3(9)(11)(14)(16) / 4.4；国际版 ToS Section 5 |

**A3 的具体违约条款（Trae，我已亲自验证原文）**：

| 条款 | 原文摘录 | 对应行为 |
|---|---|---|
| 5.2 | 「使用任何**自动化程序、软件或类似工具接入本服务**」 | 网关自动转发 |
| 5.3(9) | 「**逆向工程、拆卸、反向汇编、反编译**……试图发现……底层技术、算法、模型」 | 解密凭据、逆向私有协议 |
| 5.3(11) | 「**出租、租赁、分发、许可、转让或出售**……访问或使用本服务」 | 把额度转成 API |
| 5.3(14) | 「**以自动化的方式使用本服务**……或超出我们的任何速率限制」 | 自动化 + 并发 |
| 5.3(16) | 「**绕过我们可能用于防止或限制访问本服务的任何措施**」 | 伪造 UA / 设备指纹 |
| 4.4 | 「账号仅限于你本人使用……**禁止以任何形式赠与、借用、出租、转让、售卖**」 | 多账号池 |
| 5.4 | 「有权不事先通知你……**警示、限制功能、暂停或终止向你提供服务**」 | 处置后果 |

国际版 ToS（Section 5 Prohibited Uses）：「**autonomously or programmatically extract output**」「**rent, lease, distribute, license, transfer, or sell the Services**」「**reverse engineer, disassemble, decompile**」。

WorkBuddy 侧同理：《腾讯云代码助手软件许可及服务协议》8.2 明列禁止 (2) 反向工程 (4) 复制/修改运行数据 (5) 伪造指令变动功能 (6) **通过非授权第三方软件登录或使用本软件** (7) 干扰。

> **这不是「灰色地带」**。开源项目普遍写「仅供学习研究」，`NextAgentX/qoder-trae-workbuddy-switch` 直接用 PolyForm Noncommercial 许可并明写「不得转售、转租、共享额度」——社区自己也清楚这是违约的。

### 10.1.1 A2 是更值得先做的合规路径

如果用户的目标是「用上 Trae / WorkBuddy 的额度而不花 API 费」，A2 完全够用且不违约：

| 产品 | A2 的合规通道 | 形态 | 工程量 |
|---|---|---|---|
| **Trae** | **TRAE CLI 2.0 + ACP**：`traecli acp serve`，Zed Agent Client Protocol，stdio subprocess | 官方支持的可编程通道；参数 `--permission-mode`、`-c 'model="<id>"'`、`-c 'model_reasoning_effort="high"'`；环境变量 `TRAECLI_PERSONAL_ACCESS_TOKEN`（PAT）、`TRAECLI_HOST`、`TRAE_HOME` | 中（需写 ACP ↔ OpenAI 适配层，stdio 长连接 + 流式事件映射） |
| **WorkBuddy** | **官方「自定义模型」**（方向相反：让 WorkBuddy 调网关） | 设置 → 模型 → 自定义模型 → 填网关的 Base URL + Key；配置存 `workbuddy/models.json`（仅本地） | **零**（网关本来就是 OpenAI 兼容的） |

> **A2 的 WorkBuddy 侧是零成本的**：我们的网关已经暴露 `/v1/chat/completions`，用户直接在 WorkBuddy 里填网关地址即可。这甚至不需要写代码，只需要文档。
>
> **A2 的 Trae 侧需要写 ACP 适配层**，但它换来的是：合规、不怕封堵、官方接口不会变。这比 A3 的「逆向 + 持续对抗风控」长期成本更低。

### 10.2 设计上的自律边界

本方案**主动**排除以下能力，不做成「开关」，而是**不实现**：

1. ❌ 自动签到 / 自动领取额度 / 成长任务自动化
2. ❌ 设备指纹伪造（生成假的 `x-device-id` / `X-Machine-ID`）
3. ❌ IP 轮换 / 代理池规避风控
4. ❌ 批量注册账号
5. ❌ **身份句伪装**（把 `Claude Code` / `Codex` 文本替换成 `WorkBuddy` 以冒充官方客户端）
6. ❌ 把订阅额度对外**售卖**（网关自身是「多虚拟模型负载均衡」，但 `/api/clients` 已限制 loopback，说明项目定位是本机工具）

**保留**的：把用户**自己的、已付费的**账号额度，在**自己的机器上**用标准 API 形态调用。这是社区项目普遍接受的范围（`wangqi233/trae2api` 的免责声明也是这个口径）。

> **一条重要的自我约束**：§2.7 里列的风控对抗点，本方案**只做「如实携带」的部分**（用户提供的凭据、用户自己客户端的设备标识、正确的 UA 与必需头），**不做「伪造」的部分**（生成假指纹、伪装客户端身份）。
> 这条界线的实际效果是：**本方案可能无法通过上游的部分风控检查**，从而导致请求失败。这是**有意接受的代价**——为了「让它一定能通」而去伪造设备指纹、伪装客户端身份，性质就从「用自己账号的额度」变成了「规避风控」，那是另一回事。
>
> 如果用户希望更强的可用性，正确做法是**用官方的自定义模型接入**（§2.5 的路线 A 反方向：让 WorkBuddy 调网关），而不是把网关做得更像一个假客户端。

### 10.3 UI 上的提示

A3 类平台（`adapter = Trae / WorkBuddy`）在创建时**必须**显示警示条（不可跳过、不可记忆关闭），并**必须勾选确认**才能保存：

> ⚠️ 该上游未提供面向第三方的公开 API。此接入方式通过模拟官方客户端协议访问你本人账号的订阅额度，
> **可能违反其服务条款**（Trae 用户协议 5.2 / 5.3(9)(11)(14)(16)、腾讯云代码助手协议 8.2），
> 可能导致账号被限制或封禁。请自行评估风险，勿用于转售或共享。
>
> ☐ 我已阅读并理解上述风险，自愿承担由此产生的后果

**勾选前的「保存」按钮保持禁用**（`disabled`），且禁用态要有正确的对比度（见 `console.css` 里 Mantine 禁用态按钮的 `!important` 修正）。

警示条要有 `role="note"` + `--warn` 高对比度，确保 axe 能过。文案进 i18n（zh / en）。

**另外两处必须提示的地方**：

1. **平台列表**：A3 平台行加「实验性」`Badge`（`gray` 色），鼠标悬停显示「上游可能随时收紧，接口失效属预期风险」
2. **平台详情**：显示「最近 24h 错误码分布」。若出现 `4010` / `4015` / `4017`，顶部显示红色横幅「检测到上游风控，建议暂停使用该账号」

### 10.4 上游变更的应对

私有协议会变（客户端升级 → 头部 / 路径 / 参数变化）。设计上要能**优雅降级**：

- 适配器版本号写进日志（`trae_adapter_version=1`）
- 上游返回未知错误码时，**原样透传 + 记 warn**，不要猜
- 平台详情页显示「最近 24h 该平台的错误码分布」，便于用户发现「上游改了」
- 不做「自动升级适配器」——那需要热更新代码，安全风险大于收益

---

## 11. 代码落点清单

### 11.1 后端

| 文件 | 改动 |
|---|---|
| `src/db/schema.rs` | V28：`platforms.adapter`、`platforms.credit_unit`、`platform_credentials` 表、`models.credit_multiplier`；`CURRENT_SCHEMA_VERSION = 28`；迁移单测 |
| `src/models/platform.rs` | `Platform.adapter`、`Platform.credit_unit`、创建/更新请求字段；`PlatformAdapter` 枚举；`CreditUnit` 枚举 |
| `src/models/key_link.rs`（新） | `KeyLink` 结构 + 注册表（§7.2 的 20 条） |
| `src/models/model.rs` | `Model.credit_multiplier`；创建 / 更新 / 批量 / CSV 字段 |
| `src/models/route.rs` | `Backend` 读取 `credit_multiplier`（经 `db::route` JOIN） |
| `src/db/platform.rs` | 读写 `adapter` / `credit_unit`；`INSERT`/`UPDATE` 语句加列 |
| `src/db/credential.rs`（新） | `upsert` / `get` / `delete` / `mark_refreshed` / `mark_failure` / `list_due`；AES-GCM 加解密 |
| `src/db/model.rs` | 读写 `credit_multiplier`；批量 / CSV |
| `src/db/credit.rs` | `calibrate()`：按上游权威值调整 `spent` 并写 `calibrate` 流水 |
| `src/db/route.rs` | `BACKEND_FROM` 增加 `models.credit_multiplier` |
| `src/proxy/upstream/mod.rs`（新） | `PlatformAdapter` 判定 + `ForwardPlan` + `build()` 统一入口 |
| `src/proxy/upstream/trae.rs`（新） | Trae 请求构造、头部、SSE 解析、错误码 |
| `src/proxy/upstream/workbuddy.rs`（新） | WorkBuddy 请求构造、指纹剥离、role/tool_choice 归一、SSE 解析、错误码 |
| `src/proxy/upstream/error_map.rs`（新） | 错误码 → `UpstreamFailure` |
| `src/proxy/handler.rs` | `build_forward_request` 改调 `upstream::build`；注入 `plan.headers`；`force_stream`；错误码映射接入重试判定；积分乘数 |
| `src/cache.rs` | `ResolvedPlatform` 带解密后的凭据；`refresh()` 时重新解密 |
| `src/catalog.rs` | `catalog_key` 的 `type_key` 纳入 `adapter` |
| `src/api/platform.rs` | `adapter` 校验；credentials CRUD；`quota-sync`；`key-links`；`presets` 带 `key_link` |
| `src/api/mod.rs` | 注册新路由 |
| `src/maintenance.rs` | 凭据刷新任务 + 额度同步任务 |
| `src/main.rs`、`src-tauri/src/lib.rs` | 启动两个新维护任务（**两边都要改**，server setup 是重复的） |
| `src/models/config_transfer.rs` | 导出 / 导入带 `adapter` 与 `credit_multiplier`；**凭据不入导出**（含明文风险） |

### 11.2 前端

| 文件 | 改动 |
|---|---|
| `src/types/platform.ts`（或 `src/api.ts` 内联类型） | `adapter`、`credential` meta、`KeyLink`、`upstream_remaining` |
| `src/api.ts` | `getKeyLinks`、`getCredential`、`putCredential`、`deleteCredential`、`refreshCredential`、`syncQuota` |
| `src/components/ProviderCreateModal.tsx` | 上游类型 Select、凭据面板、警示条、申请 Key 按钮 |
| `src/pages/Platforms.tsx` | 列表「额度（上游）」列 |
| `src/pages/ProviderDetail.tsx` | 凭据卡片、额度同步卡片、错误码分布 |
| `src/presets.ts` | 新增 Agnes（国内/海外）、ModelScope、Trae（国内/海外）、WorkBuddy（国内/海外）预设 |
| `src/i18n.ts` | 中英全部新键 |
| `src/app/console.css` | 警示条 / 凭据面板的少量样式（尽量复用现有 class） |

---

## 12. 测试与验收

### 12.1 单元测试（`cargo test`）

1. **迁移**：V28 后 `CURRENT_SCHEMA_VERSION` 一致；新列 / 新表存在；旧库升级不丢行；`adapter` / `credit_unit` 为 NULL 的旧行解析出正确默认值
2. **加密往返**：`secret_blob` 加密 → 解密 → 原文一致；篡改密文 → 解密失败（不 panic）
3. **凭据脱敏**：`GET /api/platforms/{id}/credentials` 的响应里**不含**任何 token 子串
4. **WorkBuddy 请求体改写**：
   - 首条非 system → 自动插入 system
   - `role: developer` → `system`
   - `tool_choice` 对象 → 字符串（auto / required / function name / none 四种）
   - **指纹剥离**：含 `You are Claude Code, Anthropic's official CLI for Claude.` 的 system → 剥离后不含该句、其余内容保留；整条 system 就是指纹 → 用 fallback 兜底非空
5. **Trae content 归一**：字符串 content → 数组形态
6. **错误码映射**：`4001`/`1005` → 不换号；`4008` → 换号 + 冷却；`4017` → 长冷却；`Transport` → 重试；**Agnes `402` → `QuotaExhausted` 且对外回 429**
7. **`catalog_key`**：同 base_url + 同 adapter → 同 key；同 base_url + 不同 adapter → **不同** key
8. **积分乘数**：`credit_price=2` × `multiplier=0.79` = 1.58；`multiplier=0` → 记 `credits=0` 但仍计数
9. **额度校准**：上游值 < 本地值 → `spent` 上调 + 写 `calibrate` 流水；偏差 ≤ 1 → 不动
10. **`credit_unit` 分流**：
    - `credits` → 走 `try_reserve`，耗尽返回 429
    - `currency` → **不进**积分台账，`credits` 字段为 NULL，`cost` 正常计算
    - `requests` / `none` → 不建台账、不拦请求

### 12.2 E2E（`e2e/`，pytest + Playwright）

遵循现有 harness 规则：

1. **Key 申请链接**：创建弹窗里选中 ModelScope → 「申请 API Key」按钮存在、`href` 正确、`target=_blank`、`rel` 含 `noopener`；`verified != ok` 时角标存在
2. **上游类型切换**：选 Trae → 凭据面板出现、Base URL 变只读、警示条可见；切回 OpenAI → 三者复原
3. **凭据写入**：填入 token → 保存 → 重开弹窗 → 字段为空但状态显示「有效」（**明文不回填**）
4. **a11y**：新弹窗在 dark / light 双色下 axe 0 violations（注意：禁用态按钮的对比度陷阱，`console.css` 已有 `!important` 修正；按钮刚由禁用转启用时要等 `!button.disabled` + settle tick 再跑 axe）
5. **布局**：1280 / 1440 / 1920 × dark/light 无横向溢出、警示条不被裁切

> E2E 的 mock 上游需要新增两个桩：`/v2/chat/completions`（SSE）与 `/v2/billing/meter/get-user-resource`（JSON）。现有 fixture 已有 mock upstream 的机制，扩展即可。

### 12.3 手工验收清单

- [ ] Agnes 平台：填 `https://apihub.agnes-ai.com/v1` + 真实 Key → 「测试连接」通过 → 同步模型 → 打一次真实请求成功
- [ ] 魔搭平台：`https://api-inference.modelscope.cn/v1` + Key → 同步出 37 个模型（含带 `/` 的 id）→ 请求成功
- [ ] WorkBuddy 平台：导入凭据 → 同步模型目录（含 `credits` 倍率落库）→ 打一次流式请求，客户端看到完整回答
- [ ] WorkBuddy：额度同步按钮 → 显示上游剩余额度 → 与官方客户端显示一致
- [ ] Trae 平台：导入凭据 → 打一次请求 → 国内号与海外号分别验证（**模型名不通用**：用错区应得到 400，且**不触发换号**）
- [ ] 多账号：同一区配两个 Trae 账号 → 打满一个的额度 → 自动切到第二个（`BackendSelector` 轮转 + `4008` 冷却）
- [ ] 全部耗尽：返回 429 + 下次重置时间，不静默发到已超额账号
- [ ] 凭据失效：手动改坏 token → 请求得到 401 → 平台置 `Disabled` → 通知中心出现告警

---

## 13. 交付顺序

### 13.1 推荐路径（B + C + A2，全部合规）

| 阶段 | 内容 | 预估 | 风险 |
|---|---|---|---|
| **S1** | Key 申请入口注册表 + 前端按钮（§7、§9.1-3） | 0.5 天 | 无 |
| **S2** | Agnes（国内/海外）/ 魔搭 / Trae / WorkBuddy 平台预设（`presets.ts` + `platform.rs`） | 0.5 天 | 无 |
| **S2.5** | Agnes 的 `402 → QuotaExhausted` 映射（§5.6） | 0.2 天 | 无 |
| **S3** | 文档：WorkBuddy 接入网关的配置指南（A2 的 WorkBuddy 侧，零代码） | 0.2 天 | 无 |
| **S4** | **Trae ACP 适配层**：`traecli acp serve` ↔ OpenAI（A2 的 Trae 侧） | 2~3 天 | 无（官方通道） |

**S1~S3 合计约 1.5 天**，就能满足用户「用 Agnes / 魔搭的模型 + 快捷访问 Key 申请页 + WorkBuddy 用上网关模型」三个诉求，且**零合规风险**。

**S4（Trae ACP）** 是 A2 的核心，2~3 天换「合规地用上 Trae 的额度」。

### 13.2 可选路径（A3 逆向反代，需用户明确知情）

仅在用户读完 §10.1 后仍明确要求时执行：

| 阶段 | 内容 | 预估 | 稳定性 |
|---|---|---|---|
| **T1** | V28 迁移：`adapter`、`credit_unit`、`credit_multiplier`（不含 credentials） | 0.5 天 | — |
| **T2** | `platform_credentials` 表 + 加密 + CRUD API + 前端凭据面板 | 1.5 天 | — |
| **T3** | `PlatformAdapter` + `trae.rs`（content 数组归一、tools 两坑、SSE、双区） | 2 天 | 中 |
| **T4** | 错误码分类器 + 冷却策略 + IP fail-fast + 告警 | 1.5 天 | — |
| **T5** | 上游额度同步 + 校准流水（`ide_user_ent_usage` / `get-user-resource`） | 1 天 | — |
| **T6** | `workbuddy.rs`（指纹剥离、role/tool_choice 归一、SSE）—— **标为实验性** | 2 天 | **低**（上游在封堵） |
| **T7** | 测试补齐 | 1.5 天 | — |

**Trae 在 WorkBuddy 之前**（T3 早于 T6）：Trae 生态仍活跃（未见 WorkBuddy 式整体封堵），WorkBuddy 的两个主力反代项目已在 2026-10 因上游封堵停止/降低维护。

**WorkBuddy 发布时必须加显式标记**：UI 上「实验性」角标 + 创建时额外提示「该上游正在收紧非官方客户端访问，随时可能失效」。这不是免责话术，是真实的技术状态。

> **一个必须说清楚的判断**：A3 的 T1~T7 合计约 10 天，且是**持续维护**的负担（上游改协议就得跟）。而 A2 的 S4 约 2~3 天，且官方通道不会变。
> **从工程投入产出比看，A2 明显优于 A3。** 除非用户明确需要「标准 OpenAI 接口」这个形态本身（比如要让多个不支持 ACP 的客户端共用），否则不建议走 A3。

---

## 14. 风险与取舍

| 风险 | 影响 | 缓解 |
|---|---|---|
| **🔴 违反 ToS 导致封号** | 用户损失订阅费，且是本方案的直接后果 | ① **优先推 A2 合规路径**（§10.1.1）；② A3 默认关闭、需勾选确认（§10.3）；③ 不自建风控规避（§10.2）；④ 文档如实写明条款号 |
| **上游主动封堵非官方客户端**（WorkBuddy 已于 2026-10 开始） | A3 平台整体失效，投入白费 | ① **WorkBuddy 延后且标实验性**（§2.6、§13.2）；② 适配器版本号写日志；③ 未知错误码原样透传 + 错误码分布；④ 不自动升级 |
| **私有协议随客户端升级失效** | A3 平台全部 500/400 | 同上；Trae 优先（生态活跃、有完整实测文档） |
| **Trae 积分是 31 天钱包，不是自然月配额** | 用 `credit_window='month'` 会在月初多算额度 → 超额请求 → `4008` | 用 `credit_unit='requests'` + 上游 `ide_user_ent_usage` 定期同步（§4.6） |
| **Trae 4010/4015/4017 被当普通错误重试** | 继续换号会**放大封禁**，从单账号变成 IP 段被封 | 错误码分类器必须区分「风控」与「配额」；IP 级 fail-fast（同出口 60s 内 ≥2 个不同账号 403 → 停轮转） |
| **Trae `4011` 语义漂移** | UA 不对也会误报「超出限流」，导致误判 | 必须配合 UA 校验判定，不能单独当限流处理 |
| **`encrypt_key` 用默认值** | 凭据等于明文存储 | UI 一次性警告；文档写明部署时必须改（既有问题） |
| **本地 token 估算不准** | 长请求撞 400 再重试，多等 20–30 秒 | 第一版只做「撞了就收缩重试一次」；完整压缩方案另立需求（§5.7） |
| **`ClientError` 被误当可重试** | 一个「模型不在目录」被所有账号轮流撞，白等 N × 超时 | 单测覆盖（§12.1 第 6 条）；`error_map` 的 `ClientError` 明确不进入重试 |
| **Agnes 的 402 被当永久失败** | 额度耗尽时不换后端、不进台账，客户端收到看不懂的 402 | `error_map` 把 402 映射为 `QuotaExhausted`，对外回 429（§5.6） |
| **`catalog_key` 没纳入 adapter** | Trae 与 WorkBuddy 若 base_url 相同会误共享目录 | 单测覆盖（§12.1 第 7 条）；`type_key` 改为 `type:adapter` |
| **凭据明文驻留内存** | 内存转储可读 | 与现有 `platforms.api_key` 同等级；代码注释写明取舍；不落日志 |

---

## 15. 待确认问题（落地前必须澄清）

| # | 问题 | 影响 | 怎么确认 |
|---|---|---|---|
| 1 | **🔴 你选 A1 / A2 / A3 哪条路？** | 决定整份文档的执行范围 | **直接问用户**（见下方 15.1） |
| 2 | **你手上实际有哪些账号/Key？** | 决定优先级（若只有 Agnes，A 类可以整体不做） | **直接问用户** |
| 3 | **Trae 的 `x-app-id` 是固定值还是每产品不同？** | A3 的头部构造 | 抓一次 Trae 客户端请求；或读 `wangqi233/trae2api` 的 `src/trae-client.js` |
| 4 | **Trae 的 `x-device-id` / `x-machine-id` 在凭据里的确切位置** | A3 凭据导入表单的字段 | 读客户端 `storage.json` 解密后的结构 |
| 5 | **WorkBuddy 国内站是否也支持 `intl-work` 那套端点？** | A3 的站点预设表 | 用国内账号试 `copilot.tencent.com/v2/chat/completions` |
| 6 | **魔搭的免费额度规则（每日次数）** | 决定 `credit_unit='requests'` 的限流阈值 | 官方 limits 页（客户端渲染，需浏览器） |
| 7 | **Trae ACP 的实际协议细节**（A2 的 S4） | 决定 ACP 适配层的工作量 | 装 `traecli` 后跑 `traecli acp serve`，用 Zed 或手写 stdio 客户端探 |

### 15.1 给用户的一句话（决策点）

> 你要的「用 Trae / WorkBuddy 的积分调模型」，有三条路：
>
> - **A1 只做 Agnes / 魔搭 + Key 快捷入口**：约 1.5 天，零风险，官方支持。**如果你只是想「少花 API 钱」，这条就够了。**
> - **A2 走官方通道**（Trae CLI + ACP / WorkBuddy 自定义模型）：约 2~3 天，零风险。**同样用上额度，且不怕封。**
> - **A3 逆向反代**：约 10 天 + 持续维护，**明确违反 ToS**（Trae 用户协议 5.2/5.3(9)(11)(14)(16)/4.4、腾讯云代码助手协议 8.2），有封号风险。
>
> 我的建议是 **A1 + A2**。A3 我也可以做（本文档 §3~§9 就是完整设计），但需要你明确知道条款风险后再决定。

### 15.2 已在本文档中解决的原待确认问题（留档，避免重复调查）

| 原问题 | 结论 | 证据 |
|---|---|---|
| Agnes 国内站是否也用 `apihub.agnes-ai.com`？ | **否**，国内站官方 base_url 是 `https://api.agnes-ai.cn/v1` | 国内站官方 `overview.md` 明写；实测该域名 `/v1/models` 返回 401（端点存在） |
| Agnes 是否有独立的国内 apihub 域名？ | **有官方域名 `api.agnes-ai.cn`**（不是 `apihub.agnes-ai.cn`） | 同上。`apihub.agnes-ai.cn` 虽可解析但官方从未提及，属实现细节，**不写进配置** |
| Agnes 的 Key 申请精确路径？ | `https://platform.agnes-ai.com/settings/apiKeys`（海外）/ `https://platform.agnes-ai.cn/settings/apiKeys`（国内） | 控制台 Next.js 未知路径返回真 404，从 JS chunk 路由表读到后逐条实测 200 |
| Agnes 额度耗尽的错误码？ | **402**（不是 429） | 官方错误码页；网关需单独映射（§5.6） |
| 魔搭的 Key 页是否改版？ | **是**，`/my/myaccesstoken` → `/my/settings/token`（前端 redirect） | 下载 `umi.js` grep 路由表；两个地址都能打开 |
| 「agens」到底是谁？ | **Agnes AI**，`agnes-ai.com` / `agnes-ai.cn`。注意 `agnesai.com` 是停放页，不是该产品 | 本仓库已有 `agnes` 代码痕迹 + 官方文档模型名 `agnes-3.0-flash` 吻合 |
| 「workbuddy」到底是谁？ | **腾讯 WorkBuddy**（与 CodeBuddy 积分共享） | `workbuddy.cn` / `workbuddy.ai` / 腾讯云产品页 |
| 「魔塔社区」是什么？ | **魔搭**社区 ModelScope | `modelscope.cn` 域名即「魔搭」 |
| WorkBuddy 反代当前是否可用？ | **上游正在收紧**，两个主力项目已停止/降低维护 | 见 §2.6，README 原文引用 |
| Trae 反代是否有 WorkBuddy 式封堵？ | **没有整体封堵**，但风控已产品化（官方错误码含 `4010` 风险账号登出、`4015` 账户/IP 风险阻止） | 17 个仓库仅 1 个 archived（2025-06 老项目）；前 6 名 2026-10 全有提交；README 关键词「封堵/停更/不再维护」全 0 |
| Trae 的积分是「每月配额」吗？ | **不是**，是 **31 天有效期的钱包**；免费用户靠每日签到 100 续命 | 官方 `docs.trae.cn/ide_plans-and-billing` |
| Trae 有合规的编程通道吗？ | **有**：`traecli acp serve`（Zed ACP，stdio），官方文档支持 | `docs.trae.cn/cli_agent-client-protocol` |
| Trae 反代的 ToS 定性？ | **明确违约**（三重：逆向 + 自动化 + 分发），不是灰色 | 用户协议 5.2 / 5.3(9)(11)(14)(16) / 4.4；国际版 ToS Section 5。**我已通过官方 `terms/get` 接口亲自验证原文** |

---

## 16. 附：调研方法与可信度声明

本设计文档的所有「已核实」结论，均通过以下方式取得（`web_search` / `web_fetch` 工具在本会话全部 403，故改用 `pwsh` 直连 HTTP）：

| 手段 | 用途 | 示例 |
|---|---|---|
| `Invoke-WebRequest` + 剥标签 | 抓网页正文 | 官方文档、定价页 |
| `Invoke-RestMethod` + GitHub API | 查仓库 star / 更新时间 | `api.github.com/search/repositories` |
| `raw.githubusercontent.com` | 抓 README / 源码原文 | `.../trae2api/main/docs/PROTOCOL.md` |
| **Mintlify 的 `.md` 后缀与 `llms.txt`** | 抓 Agnes 文档的原始 Markdown | `wiki.agnes-ai.com/llms.txt`、`.../quickstart.md` |
| **官方渲染接口**（绕过 SPA） | 抓 Trae 用户协议全文 | `POST api.trae.com.cn/cloudide/api/v3/trae/terms/get` |
| 状态码探活 | 判断端点是否存在 | 不带 Key 请求 `/v1/models`，**401 比 200 更能说明端点正确** |

**验证外链时踩到的两个相反的坑**（已写入 §7.3）：
- ModelScope 是 umi SPA，**任意 `/my/xxx` 都返回 200** → 状态码不能证明路径有效，需 grep `umi.js` 路由表
- Agnes 控制台是 Next.js，**未知路径返回真实 404** → 可反向确认有效路径

**明确标注为「未核实」的项**（不要当事实用）：
- Trae token 的具体有效期（`expiredAt` 存在，无公开实测值）
- Trae 国际版 usage → 美元的折算公式（`docs.trae.ai` 全站 SPA）
- Trae CN 是否另有「每日/每月重置的免费配额」（`4011` 提「今日用量已达上限」但无阈值）
- Trae `4001` 的官方语义（官方文案「请稍后重试」与社区实测 `param is invalid` 冲突，疑为兜底文案）
- WorkBuddy `workbuddy/models.json` 的 JSON schema（官方未公开，需抓包）
- WorkBuddy 官方错误码页**没有**「积分耗尽」专属码，耗尽判定只能靠社区码表（`402` / `429+14018`）
- 魔搭的每日免费额度具体数字（官方 limits 页客户端渲染，未读到原文）
- OpenAI Key 申请页只拿到 **403**（Cloudflare 反爬），URL 正确但**未实测到 200**

---

## 17. 实施记录（A3 已落地）

本节记录实际实现与 §3~§13 设计的差异，供后续维护者对照。

### 17.1 已实现

| 阶段 | 内容 | 落点 |
|---|---|---|
| T1 | V28 迁移 | `src/db/schema.rs`：`platforms.adapter` / `credit_unit`、`models.credit_multiplier`、`platform_credentials` 表 |
| T2 | 凭据保险箱 | `src/crypto.rs`（AES-256-GCM）、`src/db/credential.rs`、`src/api/credential.rs` |
| T3 | 适配层 | `src/models/adapter.rs`（`PlatformAdapter` / `CreditUnit`）、`src/proxy/upstream/{trae,workbuddy}.rs` |
| T4 | 错误码分类 | `src/proxy/upstream/error_map.rs` |
| T5 | 主链路接入 | `src/proxy/handler.rs`、`src/proxy/upstream/private_sse.rs`、`src/cache.rs` |
| T6 | Key 快捷入口 | `src/models/key_link.rs`、`GET /api/platforms/key-links` |
| T7 | 前端 | `presets.ts`、`ProviderCreateModal.tsx`、`i18n.ts`、`console.css`、`api.ts` |

新增 11 个文件约 3480 行，118 个单元测试；`cargo test` 266 项全通过，前端构建通过。

### 17.2 实施中发现并修正的设计问题

1. **`adapter` / `credit_unit` 的 `key_str()` 与 `resolve()` 必须大小写一致**。
   `key_str()` 写小写（`"trae"`），而 serde 的 `rename_all = "PascalCase"` 只认 `"Trae"`——
   「存进去再读出来」会静默退化成默认值，私有协议上游被当成 OpenAI 兼容上游发请求。
   已改为大小写不敏感解析，并加往返测试（`adapter_key_roundtrips_through_resolve`）。

2. **`catalog_key` 只有私有 adapter 参与哈希**。若把 `OpenAI` / `Anthropic` 也塞进去，
   所有存量行的 key 会变化，升级后模型目录会「凭空消失」。已加回归测试
   `standard_adapter_does_not_change_legacy_key`。

3. **「没写 unit 但给了预算」必须落成 `credits`**。V28 之前「有 `credit_budget` 就是积分平台」，
   若在创建/更新时强制要求显式声明 unit，升级后所有存量平台的编辑请求都会被拒。

4. **`CreditUnit` 最终判定表与初版设计不同**（见 §4.6 的修正版）：
   真正能用 `credits` 台账的只有 WorkBuddy。Trae 国内站是 31 天钱包、Agnes 国内是货币
   （1000 Credits = ¥1）、Agnes 海外是 5 小时窗口——都不是自然窗口，分别用
   `requests` / `currency`。

5. **私有协议上游总是回 SSE，与客户端要不要流式无关**。
   初版把「上游总是 SSE」和「客户端要流式」混成一个标志，导致非流式客户端收到 SSE 正文。
   已拆成两条独立判断：`is_stream`（客户端意图）与 `private_protocol_kind`（上游形态）。

6. **聚合出的响应必须带 `created` 字段**。缺了它，网关自己的 `openai::parse_response`
   报 `missing field created`，跨协议（→ Anthropic）转换整条失败，`/v1/messages`
   会静默退回 OpenAI 形态。已加回归测试。

7. **平台不存在时写凭据要回 404 而不是 500**。`db::platform::get` 找不到行返回
   `QueryReturnedNoRows`（被转成 500），这是客户端错误。

### 17.3 端到端验证结果（真实 HTTP）

用 mock 上游模拟 WorkBuddy 的 `/v2/chat/completions`，验证了完整链路：

- 上游收到的请求：`Authorization: Bearer <token>`、`X-CodeBuddy-Request: 1`、
  `X-User-Id`、`User-Agent: WorkBuddy/5.8.3 WorkBuddy AI/5.8.3 CLI/5.8.3`、
  `stream: true`、首条消息为自动插入的 `system`
- 非流式请求 → `application/json`，内容完整聚合，`usage` 正确
- 流式请求 → `text/event-stream`，标准 `chat.completion.chunk`
- `/v1/messages` 入口 → 正确的 Anthropic 形态（`type: message`、`stop_reason: end_turn`、
  `usage.input_tokens`）
- 积分台账：`credit_price=2.0` × `multiplier=0.5` → `spent=1.0`（精确）
- 凭据元信息响应不含任何明文 token

### 17.4 本机构建注意

`cargo` 在本仓库需要包装脚本 `build.bat`（已提交）。两个必须一起解决的问题：

1. 系统 `%TEMP%` 对 `cl.exe` 不可写 → `cl: 命令行 error D8050`；
   且 `vcvars64.bat` 会把 `TEMP` 重置回系统目录，必须在它之后再覆盖。
2. **仓库内的 `target/` 目录本身也会触发同一个 D8050**（实测：改 TEMP 后仍失败，
   指向 `D:\item\ai-gateway\target` 失败、指向干净目录成功）。项目根还有一条
   `Everyone:(DENY)(DC)` ACE。所以脚本默认把 `CARGO_TARGET_DIR` 指到仓库外。

用法：`build.bat check --lib` / `build.bat test` / `build.bat build --release`。

### 17.5 T4 / T5 的最终形态（与初版设计的差异）

**T4 错误码分类器**落地时发现两个必须修正的点：

1. **流内业务错误不能走 HTTP 状态码分类**。Trae / WorkBuddy 把配额耗尽、风控这类
   业务失败放在 `200 OK` 的 SSE 里。若沿用 `classify_workbuddy(None, 200, ...)`，
   未知码会落成 `ClientError { status: 200 }`——对客户端回 200，也就是**假成功**。
   已新增 `error_map::classify_business_error(adapter, code, message)` 专用入口：
   按 adapter 查各自码表，未知码保守回 502。

2. **风控码不能被降级**。4010 / 4015 / 4017 是 Trae 的账号/IP 风控码，
   必须映射成 `RiskControl`（长冷却 + 告警 + **不换号**）。若误判成普通
   `ClientError`，网关会继续换号重试，把单账号封禁升级成 IP 段封禁。
   已有回归测试 `trae_risk_codes_stay_risk_when_classified_in_stream`。

**冷却实现**：`handler.rs` 里的进程内 `DashMap<backend_id, Instant>`，
在候选过滤阶段排除冷却中的后端。不持久化——冷却是一过性状态，重启后重新探测即可。

**T5 额度同步**：
- `db::credit::calibrate(pool, platform_id, budget, window, upstream_remaining)`
  把上游权威余额换算成 `target_spent = budget - remaining`，偏差 ≤ 1 不动（防抖动），
  超出则写 `calibrate` 流水。
- `POST /api/platforms/{id}/quota-sync` 调用上游额度接口：
  WorkBuddy `POST {base}/v2/billing/meter/get-user-resource`（`ProductCode: p_tcaca`），
  Trae `POST https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage`。
- **查询失败不污染台账**（已端到端验证：查询 500 时 `spent` 保持不变）。
