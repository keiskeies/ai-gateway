# WorkBuddy 调研报告（供 AI Gateway 接入设计使用）

> 调研时间：2026-10-27（Asia/Shanghai）
> 调研范围：WorkBuddy 身份、官方接入方式、逆向反代接口、认证与凭据、模型清单、积分机制、开源项目、合规风险、网关集成要点。
> 标注约定：**【已核实】** = 有可点击来源，且来源与结论直接对应；**【未核实】** = 社区项目实测/二手转述，未在官方文档或本机复现；**【推测】** = 基于已知事实的推断，无直接来源。

---

## 0. TL;DR（给设计文档的结论）

1. **身份已确认**：WorkBuddy 是**腾讯**出品的「全场景 AI 办公工作台」（AI Agent 办公产品，非 IDE 插件），与 **CodeBuddy（腾讯云代码助手）同属「Buddy AI」系列，积分共享、同一账号体系**。国内站 `workbuddy.cn`，国际站 `workbuddy.ai`。**不是**同名的那家美国 Field Service Management 软件（`workbuddy.com`）。**【已核实】**

2. **有两条完全不同的接入路线，合规性质天差地别**：
   - **路线 A（官方支持，合规）**：WorkBuddy 内置「**自定义模型**」功能，可把任意 OpenAI 兼容服务（含本地网关）配成它的模型后端，参数（含 API Key）只存本地 `workbuddy/models.json`。**这才是 AI Gateway 应该做的方向**——让 WorkBuddy 把请求打到 AI Gateway，而不是反过来。**【已核实】**
   - **路线 B（逆向反代，违反 ToS）**：逆向其桌面端/CLI 私有接口 `copilot.tencent.com/v2/chat/completions`，用账号 OAuth 的 accessToken 冒充官方客户端，把账号积分转成 OpenAI 兼容 API。**这条路已被上游明确风控收紧**，且违反《腾讯云代码助手软件许可及服务协议》8.2(6)。**【已核实（条款与风控收紧均有来源）】**

3. **WorkBuddy 没有面向第三方的公开模型 API**。搜索官网文档全站、腾讯云产品页，均未发现「开放 API / API Key 申请」入口。官方只提供**自家客户端 + CLI 访问权限**（付费版权益）。**【已核实（以"检索不到"为证据，见 §12.1）】**

4. **若目标是"白嫖积分"（路线 B）**：技术上完全可做（社区有 10+ 个成熟项目，最大 4561★），但**风险显著**：上游 2026-10-23 起收紧非官方客户端识别；两大主力项目已在 README 顶部标注停止/低频维护。**不建议写进产品功能，最多作为"用户自担风险的旁路"并在 UI 上明确免责。**【已核实】

---

## 1. 身份确认

### 1.1 已确认事实

| 项 | 结论 | 来源 |
|---|---|---|
| 厂商 | 腾讯（腾讯云计算（北京）有限责任公司为协议主体） | [腾讯云产品页](https://cloud.tencent.com/product/workbuddy)、[服务协议](https://www.workbuddy.cn/agreement) |
| 国内官网 | `https://www.workbuddy.cn` | 站点实测 |
| 国际官网 | `https://www.workbuddy.ai` | 站点实测 |
| 国内文档 | `https://www.workbuddy.cn/docs/workbuddy/Overview` | 站点实测 |
| 国际文档 | `https://www.workbuddy.ai/docs/zh/workbuddy/Overview` | 站点实测 |
| 产品形态 | **桌面客户端**（Windows / macOS / 统信 UOS / 银河麒麟）+ **Web 在线版** + **小程序** + **移动端** + **IDE 插件 / CLI**（同一账号体系下的多端） | [简介](https://www.workbuddy.cn/docs/workbuddy/Overview)、文档侧边栏多端切换器 |
| 定位 | 「AI Agent 办公工作台」：自然语言下达任务 → 自主拆解规划 → 调用工具 → 交付可验收产物（文档/表格/PPT/报告），支持读写本地文件、多 Agent 并行 | [腾讯云产品页](https://cloud.tencent.com/product/workbuddy) |
| 与 CodeBuddy 关系 | 「**Buddy AI**（WorkBuddy、CodeBuddy 等系列产品的统称）」，**同一账号积分共享，无需分别订阅** | [定价详情](https://www.workbuddy.cn/docs/workbuddy/Pricing) 原文：「WorkBuddy 和 CodeBuddy 同一账号积分共享，无需分别订阅。」 |
| 国际站主体名 | 文档内自称「Tencent WorkBuddy」 | [国际站简介](https://www.workbuddy.ai/docs/zh/workbuddy/Overview) |

### 1.2 同名产品排除

| 候选 | 判定 | 依据 |
|---|---|---|
| `workbuddy.com` | **不是**。「Job Management and Field Service Management Software」（美国外勤管理 SaaS） | 站点实测，title 明确 |
| `workbuddy-sy.com.cn` | **不是**。Bing 搜索结果中出现的第三方站，非官方域名 | 搜索结果，无官方背书 |
| `codebuddy.cn` / `copilot.tencent.com` | **同族**（CodeBuddy 腾讯云代码助手），与 WorkBuddy 积分共享、协议同构，但不是同一个客户端产品 | [CodeBuddy 官网](https://www.codebuddy.cn)、社区项目站点表 |

> **结论**：用户在「用积分调模型」语境下说的 WorkBuddy，**就是腾讯这个**。证据强度：高（官网 meta 直接写明厂商与产品定位）。

### 1.3 双区（国内 / 国际）是两套独立体系

- 国内版与国际版是**两套独立账号、两套独立余额**，额度不通用、模型目录不同、单价不同。**【已核实】**（[社区实测文档](https://github.com/yuan240324/workbuddy-openai-proxy/blob/main/docs/models-and-quota.md)：「国内版和国际版是两套独立账号、两份额度」）
- 国内版模型**没有** Claude / Gemini 系；国际版**有**。**【未核实】**（社区实测清单，见 §6）

---

## 2. 两条接入路线（合规分水岭）

这是本文最重要的区分。设计文档必须把这两条写成两个独立章节，不能混为一谈。

### 路线 A：官方「自定义模型」——把 WorkBuddy 当客户端，把 AI Gateway 当上游

**这是官方文档明确支持的功能，完全合规。**

| 项 | 内容 | 来源 |
|---|---|---|
| 功能名 | 自定义模型（Custom Model） | [模型配置](https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model) |
| 配置项 | Provider / API Base URL / API Key / Model Name；标准供应商还会自动写入工具调用、图片输入、推理模式等能力标记 | 同上 |
| 存储位置 | **`workbuddy/models.json`**（仅本地，不上传云端）。历史路径 `~/.codebuddy/models.json` 仍兼容，可在 UI 中查看/编辑/删除 | 同上 |
| UI 路径 | 设置 → 模型 → 自定义模型 → 添加 | 同上 |
| 内置预设 | 主流厂商预设、腾讯云 Token Plan、Coding Plan（腾讯云/智谱/Kimi）、**Ollama 本地部署**（默认 11434）、自定义/Custom | 同上 |
| **关键开关** | 高级配置里的「**自定义协议**」：关闭（默认）时使用标准 `/chat/completions` 路径并自动校验补全；**开启后直接按用户填写的 URL 发起请求，跳过路径校验与自动补全** | 同上 |

**对 AI Gateway 的直接含义**：
- AI Gateway 可以作为一个「自定义模型」供应商被 WorkBuddy 调用，Base URL 填 `http://127.0.0.1:1994/v1`，模型名填网关的虚拟模型名。
- 如果 WorkBuddy 的路径拼接与网关不匹配，**打开「自定义协议」开关直接给完整 URL** 即可绕过（如 `http://127.0.0.1:1994/v1/chat/completions`）。
- 这条路线的**费用由用户自己的第三方 Key 承担**，官方文档原文：「自定义模型产生的全部费用（Token、订阅等）由您向第三方支付」。**不消耗 WorkBuddy 积分。**

**⚠️ 注意方向**：路线 A 是「WorkBuddy 消费 AI Gateway 的模型」，**不是**「AI Gateway 消费 WorkBuddy 的积分」。用户想要的是后者。设计文档若写错方向会南辕北辙。

### 路线 B：逆向反代内置额度——把 WorkBuddy 积分转成 API

**违反 ToS，且上游正在主动封堵。**

- 原理：逆向 WorkBuddy/CodeBuddy 官方客户端与后端的私有接口，用账号 OAuth 拿到的 accessToken 冒充官方客户端发请求，从而消耗账号积分。
- 社区已有 10+ 个成熟项目（最大 4561★），见 §9。
- **上游反制已发生**：两个主力项目在 README 顶部明确写了上游收紧。**【已核实】**

  > `linguo2625469/workbuddy2api-panel`（4561★）：「**项目已停止维护（2026-10-21）**……**上游已开始禁用反代**——WorkBuddy 对非官方客户端的识别与拦截正在收紧（通道校验、账号限制等），反代通道的稳定性与存活周期已不可控」——[README](https://github.com/linguo2625469/workbuddy2api-panel)

  > `ardeyouxipianyi/workbuddy2api-hub`（3064★）：「**WorkBuddy 上游正在收紧对非官方客户端的识别与拦截。** 封禁：2026-10-23 下午起，非官方客户端的识别与拦截明显收紧。」——[README](https://github.com/ardeyouxipianyi/workbuddy2api-hub)

**结论**：路线 B 技术可行但**不适合作为产品功能**。若一定要做，应定位为「用户自担风险的本机旁路」，并在文档与 UI 上显式免责（见 §11）。

---

## 3. 路线 B：上游接口事实（host / path / header）

> ⚠️ 本节全部来自**社区逆向项目源码**，非官方文档。已尽量多源交叉验证（3 个独立项目 + 1 份官方错误码页对照），但仍属**【未核实（社区逆向）】**。**任何字段名都不要当成官方契约。**

### 3.1 双区 host 表（多源一致，证据强度：较高）

| 用途 | 国内版（cn） | 国际版（global） | 来源 |
|---|---|---|---|
| 聊天补全 / 模型目录 / token 刷新 | `https://copilot.tencent.com` | `https://www.workbuddy.ai` | [panel headers.go/client.go](https://github.com/linguo2625469/workbuddy2api-panel/blob/main/internal/upstream/client.go)、[yuan240324 config.mjs](https://github.com/yuan240324/workbuddy-openai-proxy/blob/main/src/config.mjs) |
| 备用聊天 host（国内） | `https://www.workbuddy.cn`（同套接口，海外可达） | — | [hub README §14 `WB_CN_CHAT_UPSTREAM`](https://github.com/ardeyouxipianyi/workbuddy2api-hub) |
| 计费 / 签到 / 余额 | `https://www.codebuddy.cn` | `https://www.workbuddy.ai` | panel `client.go`；yuan240324 `billingBase` |
| 国际版 CLI 站（另一套） | — | `https://www.codebuddy.ai` | yuan240324 站点表 `intl-cli` |
| Origin / Referer | `https://www.codebuddy.cn` | `https://www.workbuddy.ai` | panel `headers.go` |

> 站点表交叉验证：yuan240324 的站点键为 `cn-cli`(copilot.tencent.com) / `intl-cli`(www.codebuddy.ai) / `intl-work`(www.workbuddy.ai)，与 panel 的 `ChatBaseCN` / `defaultGlobalBase` 一致。

### 3.2 接口路径（**已多源交叉验证**）

| 相对路径 | 方法 | 用途 | 出现于 |
|---|---|---|---|
| `/v2/chat/completions` | POST | **聊天补全（SSE）** | panel、yuan240324、proxy-hub 全部一致 |
| `/console/chat/completions` | POST | 国际版旧路径（`/v2` 的等价端点） | panel 注释（#119 已固定走 `/v2`） |
| `/console/enterprises/personal/models` | GET | 动态模型目录 | panel、yuan240324 一致 |
| `/v3/config` | GET | 官方 IDE 配置目录（`agents[cli].models`，含积分倍率） | panel |
| `/v2/plugin/auth/token/refresh` | POST | token 刷新 | panel、yuan240324 一致 |
| `/v2/plugin/auth/state?platform=CLI` | POST | OAuth 设备授权第一步：取 state + authUrl | yuan240324 `device-login.mjs` |
| `/v2/plugin/auth/token?state=` | GET | OAuth 设备授权第二步：轮询取 token | 同上 |
| `/v2/plugin/login/account?state=` | GET | OAuth 取账号信息（uid / enterpriseId / nickname） | 同上 |
| `/v2/billing/meter/get-user-resource` | POST | **积分余额查询（个人版）** | panel、yuan240324 一致 |
| `/billing/meter/get-user-resource` | POST | 国际版无 `/v2` 前缀形态（404 时回落） | panel `billingMeterPaths` |
| `/v2/billing/meter/get-enterprise-user-usage` | POST | **企业版额度查询**（个人接口对企业号恒返回空 Accounts） | panel `enterpriseResource` |
| `/v2/billing/meter/daily-checkin` | POST | 每日签到 | panel |
| `/v2/report` | POST | 行为事件上报（签到/成长任务用） | panel |
| `/v2/activity/growth/tasks` | GET | 成长任务列表 | panel |
| `/console/account` | GET | Web 控制台账号资料（`x-client-platform: web`） | panel `profile.go` |
| `/agenttool/v1/search` | POST | 联网搜索工具 | panel |

> **找不到**：`/v2/responses`。CangShui 项目实测「2026-10-22 用真实短请求探测过 `https://copilot.tencent.com/v2/responses`，返回 HTTP 404 `Route Not Found`」——即上游**没有** OpenAI Responses 原生端点，必须网关侧转换。**这是设计文档必须写清的一条。**

### 3.3 认证 header（核心）

三个项目给出的 header 集合高度一致，取并集如下。**注意：不同项目对同一语义的头名有出入，说明这块是「试出来的」而非文档化的。**

**通用头**（panel `CommonHeaders` / yuan240324 `commonHeaders`）：
```
Content-Type: application/json
Accept: application/json, text/event-stream     # 聊天时
X-Requested-With: XMLHttpRequest
Origin: https://www.codebuddy.cn | https://www.workbuddy.ai
Referer: <origin>/
User-Agent: <见下>
X-CodeBuddy-Request: 1                          # panel 称"官方客户端风控闸门头，所有 API 请求必带"
Accept-Language: zh-CN | en-US                  # 按 realm 切
X-Machine-ID / X-Session-ID                     # panel 按 uid 稳定派生（非官方值，防风控关联）
```

**聊天头**（额外）：
```
Authorization: Bearer <accessToken>
X-User-Id: <uid>
X-Enterprise-Id: <enterpriseId>  或  X-No-Enterprise-Id: 1
X-Domain: <domain>  或  X-No-Department-Info: 1   # 国际版固定 X-Domain: www.workbuddy.ai
X-No-Authorization: 1 / X-No-User-Id: 1          # 缺省时的占位约定
X-Request-ID: <32 hex>
X-Conversation-ID / X-Conversation-Request-ID / X-Conversation-Message-ID
X-Trace-ID / X-Root-Request-ID / X-B3-TraceId / X-B3-SpanId / X-B3-Sampled
X-Agent-Purpose: conversation
X-IDE-Name / X-IDE-Type / X-IDE-Version / X-Product
X-Device-Token: <设备风控 token，有则发>
```

**刷新头**（**只允许出现在 refresh 请求**）：
```
X-Refresh-Token: <refreshToken>
X-Auth-Refresh-Source: plugin | workbuddy
```

**UA 形态（关键风控点）**：
- panel 默认：`WorkBuddy/<clientVer> <platform>/<clientVer> CLI/<cliVer>`，其中国内 `platform=WorkBuddy`、国际 `platform=WorkBuddy AI`；内置版本 `5.5.4` / CLI `2.137.1`。
  - 源码注释：「global 账号送错平台段（`WorkBuddy` 非 `WorkBuddy AI`）可能触发上游 403 code 11140 "request illegal" 风控。官方无任何 UA 随机化，故默认确定性。」
- CangShui 采集样本：`WorkBuddy/5.8.3 WorkBuddy/5.8.3 CLI/2.161.0`
- proxy-hub：`X-IDE-Version: 2.136.0`（较旧）
- yuan240324：`CLI/2.63.2 CodeBuddy/2.63.2`（更旧形态，仍可用）
- **`/v3/config` 对 UA 极敏感**：panel 注释「该端点对 UA 敏感：必须带 CodeBuddy/CodeBuddyIDE 版本，否则 400 code=12403」；且「不同 UA 下发不同模型集合」。

### 3.4 聊天请求体要点

- **上游拒绝非流式**：出站必须强制 `stream: true`（panel `payload.go: obj["stream"] = true`；yuan240324 同）。非流式由网关本地聚合 SSE。
- `stream_options: {include_usage: true}` 缺省补上，上游据此在末帧回 usage。
- `max_completion_tokens` → `max_tokens`（上游只认 `max_tokens`，别名透传会被忽略并回落默认 32000）。
- `developer` 角色 → `system`（上游 role 白名单不含 developer，命中即 400 code 11128）。
- `tool_choice` 对象形态 → 字符串（对象会 400 code 11101）。
- `image_url` 字符串 → `{url: ...}` 对象形态。
- GPT 系 `max_tokens < 16` 会 400 code 11133，需 clamp 到 16。
- 国际版**首条消息必须是 system**，否则 400 `first message is not system prompt`（yuan240324 实测）。
- 工具调用配对必须完整（`tool_calls` ↔ `tool` 结果），否则后续每条消息都 400。

### 3.5 积分余额查询（额度查询接口）

**个人版**（panel `CreditPackages` / yuan240324 `queryCredit` 一致）：
```
POST {billingBase}/v2/billing/meter/get-user-resource
Body: {
  "PageNumber": 1, "PageSize": 100,
  "ProductCode": "p_tcaca",
  "Status": [0, 3],
  "PackageEndTimeRangeBegin": "YYYY-MM-DD HH:MM:SS",
  "PackageEndTimeRangeEnd":   "YYYY-MM-DD HH:MM:SS"
}
响应: { code: 0, data: { Response: { Data: { Accounts: [ {
  PackageName, PackageCode, SubProductCode, SubProductName,
  CapacityRemain, CapacityUsed, CapacitySize,
  CycleCapacityRemain, CycleCapacityUsed, CycleCapacitySize,
  CycleEndTime, DeductionEndTime, CreateTime
} ] } } } }
```
- 剩余量取法：`CycleCapacitySize > 0 ? CycleCapacityRemain : CapacityRemain`（两项目一致）。
- **`ProductCode: "p_tcaca"`** 是 CodeBuddy/WorkBuddy 的产品码，**不要编造其它值**。
- 真实失效时刻看 `DeductionEndTime`（epoch 毫秒），`ExpiredTime`/`PackageEndTime` 实测恒空，`CycleEndTime` 只是周期边界。

**企业版**（panel `enterpriseResource`）：
```
POST {billingBase}/v2/billing/meter/get-enterprise-user-usage   # X-Enterprise-Id 由 BillingHeaders 注入
Body: {}
响应: {"credit":812.45, "limitNum":2000, "cycleStartTime":"...", "cycleEndTime":"...", "cycleResetTime":"..."}
```
- **语义相反**：`credit` = 本周期**已用**，`limitNum` = 分配额度 → `remain = limitNum - credit`。
- `limitNum < 0` 表示不限量。
- 企业**池**总额度成员无权查（所有池端点 403 not_authorized）。

### 3.6 没有公开的「额度查询」官方文档

官方文档《查看套餐与用量》只描述 UI 路径（设置 → 套餐与积分；个人中心 → 套餐与用量 → 积分消耗明细），**没有给任何 HTTP 接口**。**【已核实】**（[用量查看](https://www.workbuddy.cn/docs/workbuddy/Usage)）

---

## 4. 认证方式、有效期、refresh、本地凭据

### 4.1 认证方式：OAuth 设备授权（不是 API Key，不是 Cookie）

**流程**（yuan240324 `device-login.mjs`，panel 面板同）：
1. `POST {apiBase}/v2/plugin/auth/state?platform=CLI` → 返回 `{state, authUrl}`
2. 用户在浏览器打开 `authUrl` 完成登录 + **点「授权/允许」**
   - ⚠️ 实测坑：「只完成前者时上游会持续返回 `11217: login ing...`，需要在授权页点一次授权才会下发 token」
3. `GET {apiBase}/v2/plugin/auth/token?state=<state>` 轮询 → `{accessToken, refreshToken, expiresIn, domain}`
4. `GET {apiBase}/v2/plugin/login/account?state=<state>`（带 Bearer）→ `{uid, enterpriseId, nickname}`

### 4.2 有效期与 refresh

| 项 | 值 | 来源 |
|---|---|---|
| accessToken 形态 | JWT，`payload.exp` 可解出到期时间 | yuan240324 `auth.mjs::jwtClaims/hydrateFromToken` |
| 提前刷新窗口 | **5 分钟**（`Date.now() > expiresAt - 5*60*1000`） | 同上 `ensureToken` |
| 刷新接口 | `POST {apiBase}/v2/plugin/auth/token/refresh`，头带 `X-Refresh-Token` | panel、yuan240324 一致 |
| 刷新返回 | `{accessToken, refreshToken, expiresIn}`；**refreshToken 会轮换** | 同上 |
| 刷新超时 | 20s | yuan240324 |
| 并发保护 | 单飞（singleflight），避免并发刷新风暴 | 两项目均实现 |
| **反重放** | panel 注释：「上游对 refreshToken 有反重放：RT 被消费后再用会撞 12153……并发第二发会带着已被消费的 RT 撞 12153 并整链作废」 | panel `client.go:1020` |
| 会话失效标记 | `401` + body 含 `Offline user session not found` 或 `12153` | panel `sessionDeadMarkers` |
| 失效处置 | panel：连续 3 次才永久禁用（一次多为抖动）；yuan240324：401 先原地强刷再换号 | 两项目 |

### 4.3 客户端本地凭据存储位置

| 场景 | 路径 | 来源 |
|---|---|---|
| WorkBuddy/CodeBuddy 桌面端（Windows） | `%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\workbuddy-desktop.info`<br>`%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\Tencent-Cloud.coding-copilot.info` | [proxy-hub codebuddy.js](https://github.com/1416277987/proxy-hub/blob/main/adapters/codebuddy.js) |
| 反代自建凭证目录 | `./auths/workbuddy-<uid>.json`，权限 `0600` | panel `auth.go::SaveAtomic` |
| 单机版 | `auth.<site>.json`（如 `auth.cn-cli.json`）、`auth.<site>.pool.json`，权限 600 | yuan240324 |
| 客户端模型配置（官方） | `workbuddy/models.json`；历史 `~/.codebuddy/models.json` | [官方模型配置文档](https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model) |

**凭据文件 JSON 结构**（panel `auth.go::Parse`，兼容两种形态）：
```jsonc
// 嵌套形（插件 OAuth 输出）
{ "auth": { "accessToken": "...", "refreshToken": "...", "expiresAt": 1234567890,
            "domain": "www.workbuddy.ai", "realm": "global" },
  "account": { "uid": "...", "enterpriseId": "...", "nickname": "..." },
  "device_token": "..." }
// 扁平形（手写/旧版）
{ "accessToken": "...", "refreshToken": "...", "expiresAt": 0, "domain": "...",
  "realm": "cn", "uid": "...", "enterpriseId": "...", "nickname": "...", "device_token": "..." }
```

### 4.4 桌面端凭据加密（重要变化）

**【未核实（单一来源，但描述具体、可信度高）】**：`ardeyouxipianyi/workbuddy2api-hub` README 称——

> 「桌面客户端从 **2026-09-24** 起把 `accessToken` / `refreshToken` 存成 `$wbEncrypted` 信封，网关按客户端 `packages/at-rest-crypto` 的同一套方案（**AES-256-GCM + `WB-AAD` 帧头**）就地解密……解码密钥编译在客户端原生模块里、磁盘上没有明文。Windows 上可以从**正在运行的**桌面端进程内存里找回（只读 `OpenProcess(PROCESS_VM_READ)` + `ReadProcessMemory`）」

→ **含义**：如果 AI Gateway 想「从本机客户端直接导入凭据」，2026-09-24 之后的桌面端版本需要额外解密步骤，macOS/Linux 上还得用户手动粘贴密钥。**这大幅提高了路线 B 的实现成本**，是设计文档应记录的关键约束。

---

## 5. 模型清单与 id 命名

### 5.1 官方文档给出的内置模型（国内版）**【已核实】**

来源：[模型配置](https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model)

| 展示名 | 能力 |
|---|---|
| Hy4 preview（混元） | 图片输入、思考模式 |
| Hy3 | 图片输入、思考模式 |
| GLM-5.3 | 图片输入、思考模式、1M 上下文 |
| GLM-5.3-Flash | 思考模式 |
| GLM-5.2 | 图片输入、思考模式、1M 上下文 |
| GLM-5.1 | 推理模型 |
| GLM-5v-Turbo | 图片输入、推理模型（原生多模态） |
| MiniMax-M3 | 图片输入、推理模型 |
| MiniMax-m2.7 | 图片输入、推理模型 |
| Kimi-K3 | 图片输入、推理模型 |
| Kimi-K2.7-Code | 图片输入、推理模型 |
| Kimi-K2.6 | 图片输入、推理模型 |
| Deepseek-V4-Flash | 思考模式、1M 上下文 |
| Deepseek-V4-Pro | 思考模式、1M 上下文 |

另有三档模式（快速 / 均衡 / 极致）、思考强度、上下文窗口（默认 200K，可调）、Max 模式（关闭时自动压缩上下文省积分）。

### 5.2 API 侧的真实 model id（社区实测）**【未核实】**

**命名规律**：全小写、连字符分隔、`<厂商>-<版本>`，如 `deepseek-v4.1-flash`、`glm-5.3`、`kimi-k3`、`hy4-preview`。**没有厂商前缀的 `claude-*` / `gpt-*` / `gemini-*` 是国际版专有。**

**国际版（hub 列 17 个，yuan240324 实测 22 个，取交集/并集）**：
```
hy4-preview-f, hy3, deepseek-v4.1-flash, gpt-6-astra,
gpt-5.6-sol, gpt-5.6-terra, gpt-5.6-luna, gpt-5.5, gpt-5.4, gpt-5.3-codex,
grok-4.7, gemini-3.5-flash, gemini-3.1-pro, gemini-3.1-flash-image,
claude-sonnet-4.6, claude-opus-4.6,
glm-5.3, glm-5.3-flash, glm-5.2,
kimi-k3, kimi-k2.8-preview, kimi-k2.7, kimi-k2.6, kimi-k2.5,
minimax-m3, auto
```

**国内版（hub 列 14 个，yuan240324 实测 16 个）**：
```
hy4-preview, hy4-preview-x, hy3, hy3-x,
deepseek-v4-pro, deepseek-v4-flash, deepseek-v4.1-flash,
glm-5.3, glm-5.3-flash, glm-5.2, glm-5.1, glm-5v-turbo,
minimax-m3, minimax-m2.7,
kimi-k3-1, kimi-k2.8-preview, kimi-k2.7, kimi-k2.6, kimi-k2.5,
auto
```

**国内 / 国际差异**（证据强度：中，社区实测）：
- **国内没有** Claude / Gemini / GPT 系（实测 `claude-sonnet-4.6` 等在国内返回 400 `only available for authorized users`）。
- **国内有** `deepseek-v4-pro`、`minimax-m3`（hub 判定为 CN 独占）。
- 国际版 `deepseek-v4.1-flash` 实测 **0 扣费**；国内版同名模型按 `x0.03` 收费。
- 同名模型**不跨区混合轮询**（两区作为独立出口分别调度）。

**模型上下文 / 输出上限（panel `model.json` seed，示例）**：
```
glm-5.3: 1,000,000 / 131,072      deepseek-v4.1-flash: 1,000,000 / 384,000
kimi-k3: 1,048,576 / 131,072      minimax-m3: 512,000 / 512,000
gpt-6-astra: 1,050,000 / 128,000  gpt-5.3-codex: 400,000 / 128,000
gemini-3.5-flash: 1,048,576 / 65,536   auto: 168,000 / 32,000
```

**模型目录接口**：`GET /console/enterprises/personal/models`（返回含 `credits` 积分倍率的模型清单）；`GET /v3/config` 的 `agents[cli].models` 是更权威的目录源（panel 以它为准）。国际版目录接口**不稳定**（403 access_denied / 偶发 500），yuan240324 因此内置了静态清单兜底。

---

## 6. 积分 / 额度机制

### 6.1 计量方式（官方口径）**【已核实】**

来源：[积分说明](https://www.workbuddy.cn/docs/workbuddy/Credits)

> 「在 WorkBuddy 中，**所有对话相关请求均需消耗积分**。具体消耗数量因**任务复杂度**及**所用高级模型**而异。」
> 「积分的消耗量 = **模型 Token 定价** × **任务复杂度** 两个因素」
> 「模型 Token 定价：不同模型消耗的积分有所不同……任务复杂度：简单任务（如简短问答）消耗较少，而复杂任务（如长代码分析、多轮对话、知识库检索）需更多推理计算和 Token 处理」

→ **不是纯 token 线性计价**，还有「任务复杂度」这个非公开系数。**AI Gateway 无法在本地精确预算积分消耗**，只能事后从响应 `usage.credit` 读实际扣费。**【已核实 + 推测】**

### 6.2 扣减顺序与有效期（官方）**【已核实】**

- 扣减顺序：**先到期先扣**；到期时间相同则**先基础积分、后赠送积分**。
- 个人版：月会员有效期 = 新购当日 ~ 次月当日；年会员 = 自然年。
- **积分按月发放，当月有效、不累积结转**；赠送部分自到账日起 **1 个月**内有效。
- 加量包：新购当日 ~ 次月当日，过期未用自动失效。
- 会员到期后降为个人体验版额度。
- 企业版：到期后企业无法继续使用产品任何功能。

### 6.3 额度档位

**国内个人版**【已核实，[定价详情](https://www.workbuddy.cn/docs/workbuddy/Pricing)】：

| 版本 | 月付 | 连续包月(7折) | 连续包年(5.6折) | 基础积分/月 | 限时加赠 | 每月实得 |
|---|---|---|---|---|---|---|
| 体验版 | 免费 | — | — | 500 | — | 500 |
| 标准版 | 99 元/月 | 70 元/月 | 672 元/年 | 2,000 | 2,000 | **4,000** |
| 高级版 | 199 元/月 | 140 元/月 | 1,344 元/年 | 4,000 | 5,000 | **9,000** |
| 旗舰版 | 999 元/月 | 700 元/月 | 6,720 元/年 | 20,000 | 30,000 | **50,000** |

加量包（国内）：1 个月 / 1,000 积分 / 50 元，不支持退款。
其它权益：代码补全（体验版 5,000 次/月，付费无限）、自动任务数、项目数、资料库容量、**CLI 访问支持（付费版才有）**。

**国际个人版**【已核实，[intl 定价](https://www.workbuddy.ai/docs/zh/workbuddy/pricing)】：

| 版本 | 价格 | 基础积分/月 | 加赠 | 每月实得 | 活跃赠送 |
|---|---|---|---|---|---|
| Free | 免费 | 100 | — | 100 | 30 积分/天 |
| Pro | $10/月（首月 $7）、$96/年 | 1,000 | 1,000 | **2,000** | 50 积分/天 |

Team 版：$40/坐席/月，每席位每月 1,000 积分，**团队公共池共享**，最早过期优先扣减。
加量包（国际）：Pro 500 积分/$15（1 个月）；Team 2,000/$40 ~ 200,000/$4,000（6 个月）。

### 6.4 额度查询（官方 UI vs 逆向接口）

- **官方**：设置 → 套餐与积分（客户端）；个人中心 → 套餐与用量 → 积分消耗明细（官网）。**【已核实】**
- **逆向**：§3.5 的 `get-user-resource` / `get-enterprise-user-usage`。**【未核实】**

### 6.5 耗尽 / 限流错误码（关键！）

**官方错误码页**【已核实，[错误码说明](https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Error-Code)】：

| code | 官方说法 | 官方建议 |
|---|---|---|
| 1001 | 多与模型侧状态有关 | 切换模型重试 |
| 3002 / 3003 / 3007 | 多与网络环境有关 | 排查公司/家庭网络、关闭网络代理 |
| **6003** | **请求频率受限**，专业版也可能出现 | 切换模型重试 |
| **6004** | **请求频率受限**，专业版也可能出现 | 切换模型重试 |
| 11115 | **输入内容过长** | 精简输入或换更大上下文模型 |
| 11133 | 多与模型侧状态有关 | 新开会话或切换模型 |
| 11134 | 多与模型侧状态有关 | 新开会话或切换模型 |
| 14003 | 多与模型侧状态有关 | 切换模型重试 |

> **官方错误码页没有列「积分耗尽」的专属码。** 这是一个重要的空白——设计文档若需要精确的耗尽判定，只能靠社区逆向的码表 + 关键词。

**社区逆向补充的码表**【未核实，panel `client.go::Classify`】：

| code / 状态 | 语义 | 处置 |
|---|---|---|
| **402** | 真正的计费余额耗尽（最严、最不可自愈，最先判） | 硬冷却到次日 04:00，签到后余额恢复自动解冻 |
| **429 + code 14018** | **明确的账号积分耗尽** | 同 402（必须先于通用 429 判，否则被误判为限流硬冷却 12h） |
| **429 + code 6021** / `queue.full` | 渠道级容量满载（推理集群排队打满），非账号问题 | 不罚号、不轮转，末端 503 `upstream_busy` |
| 429（无 14018/6021） | 软限流 | 软冷却 600s 起指数退避，封顶 2h |
| **429 + code 6004（msg 带「将在 … 重置」）** | **模型级限流**，按上游重置墙钟冷却 | 冷却到重置时刻，豁免切模型 |
| 11128 | 通道校验失败（`request illegal`，displayTips 可能不是官方客户端发送） | **不罚账号**（账号健康） |
| **11140** | 账号被上游限制（异常行为/风控） | 账号级故障，冷却轮换 |
| 14017 | `trial not activated`（注册未完成） | 账号级故障（常带 429，必须先于 429 判） |
| 11102 | 该后端无此模型 | (账号, 模型) 负缓存避让 |
| 11158 | 该账号类型无权使用该模型 | 同上 |
| 11101 | 请求体解析失败（`Unmarshal chat params failed`） | 不罚号、**不轮转**（客户端畸形 JSON） |
| 11115 | `prompt is too long` | 请求级错误，不罚号、不轮转 |
| 11135 | 图片相关参数非法 | 请求级 |
| 12153 | session / RT 失效 | 需重新登录 |
| 12403 | `/v3/config` 缺 UA 版本 | 补齐 UA |
| 10001 / 14001 | 「今天已签到」类幂等码（**不是**签到成功专属） | — |

> ⚠️ **11128 与 11140 的 msg 都是 `request illegal`**，panel 明确警告「只看文案会把一次通道校验失败误判成账号封禁并硬禁用健康账号」，必须 **code + 文案双条件**判定。这是最容易踩的坑。

---

## 7. 已知风控与反制（做路线 B 必须知道）

**上游在主动识别非官方客户端**，社区总结的对抗点：

| 维度 | 社区做法 | 来源 |
|---|---|---|
| **UA 三段式** | `WorkBuddy/<ver> <platform>/<ver> CLI/<ver>`；国际版 platform 必须是 `WorkBuddy AI`，否则 11140 | panel `headers.go` |
| **X-CodeBuddy-Request: 1** | 所有 API 必带的闸门头 | panel |
| **X-Domain / X-No-Enterprise-Id** | 国际版固定声明 `www.workbuddy.ai` | panel |
| **会话头族完整性** | 缺 `X-Conversation-ID` 的请求会 400 / 11128（conversation id missing） | panel `headers.go` issue #190 |
| **设备指纹** | `X-Machine-ID` / `X-Session-ID` 按 uid 稳定派生（不随机漂移，防跨账号关联）；`X-Device-Token` 来自客户端 | panel |
| **system prompt 指纹** | 客户端 system 模板句被上游**逐字精确匹配**后 400（`blocked by security policy` / `unapproved channel` / `illegal api invocation`）→ 网关需替换或剥离客户端 system | panel `prompt.mode`、yuan240324 `stripClientFingerprint` |
| **消息内容指纹脱敏** | 出站 body 黑名单字段清洗（客户端名/版本/路径等） | panel `sanitize.go` |
| **身份句改写** | `Claude Code` / `Codex` / `DeepSeek Harness` → `WorkBuddy` 文本级替换 | CangShui `identity` 配置 |
| **并发限流** | 国际版 WAF 更紧，单账号在途并发需压低（panel 默认 `max_in_flight_global: 2` vs 国内 3） | panel |
| **请求体体积** | 上游按请求体积（非 token 数）触发 400，约 34~40 万字符；报错里的 token 数字**不可信** | yuan240324 §6 |
| **`/v2/responses` 不存在** | 探测返回 404 `Route Not Found` | CangShui `outbound-compatibility.md` |
| **企业号无成长体系** | 签到/成长任务/连登/旅行对企业号一律被拒 | panel `auth.go::IsEnterprise` |
| **企业切换会话** | `/v2/report` 返回 200 但**静默丢弃**（读得见、记不上账） | panel `IsEnterpriseSession` |

**上游收紧的时间线**【已核实】：
- 2026-09-24：桌面端凭据改为 `$wbEncrypted` 加密信封
- 2026-10-21：`workbuddy2api-panel`（4561★）宣布停止维护，理由「上游已开始禁用反代」
- 2026-10-23：`workbuddy2api-hub`（3064★）公告「下午起上游识别与拦截明显收紧」，转低频维护

---

## 8. 开源反代项目清单（GitHub API 实测，2026-10-27）

| 仓库 | ★ | Fork | 最近提交 | 语言 | 定位 / 备注 |
|---|---|---|---|---|---|
| [linguo2625469/workbuddy2api-panel](https://github.com/linguo2625469/workbuddy2api-panel) | 4561 | 1575 | 2026-10-21 | Go | **最完整的参考实现**（fork 自已删库的 `Sliverkiss/workbuddy2api`）。多账号池、熔断冷却、会话粘性、Web 面板、成长任务自动化。**已停止维护** |
| [ardeyouxipianyi/workbuddy2api-hub](https://github.com/ardeyouxipianyi/workbuddy2api-hub) | 3064 | 721 | 2026-10-24 | Python | 双区（intl/cn）独立路由、Responses API、Anthropic 实验适配（默认禁用）、一键配置 6 个客户端。**低频维护** |
| [babygoton/WorkDaddy](https://github.com/babygoton/WorkDaddy) | 2327 | 256 | 2026-10-24 | JS | 桌面端增强助手（签到/多账号/免打扰/会话迁移），非网关 |
| [ithtelab/workbuddy-manager](https://github.com/ithtelab/workbuddy-manager) | 1948 | 440 | 2026-10-27 | Python | workbuddy2api 的 Web 管理端（Next.js + FastAPI），**仍在活跃** |
| [88lin/workbuddy-auto-signin](https://github.com/88lin/workbuddy-auto-signin) | 1670 | 159 | 2026-10-26 | Python | 自动签到领积分，非网关 |
| [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) | 1561 | 212 | 2026-10-12 | Rust | Tauri 账号切换 + 积分到期监控 + Token 统计 |
| [aimod-cc/Agent2API](https://github.com/aimod-cc/Agent2API) | 1492 | 386 | 2026-10-26 | Rust | **17 家提供商**统一网关（WorkBuddy/Trae/Qoder/CatPaw…），协议最全（Chat/Responses/Messages），**活跃** |
| [misakano7545/workbuddy2api-panel](https://github.com/misakano7545/workbuddy2api-panel) | 577 | 143 | 2026-10-27 | Go | 上面 panel 的活跃 fork |
| [CangShui/workbuddy-gateway](https://github.com/CangShui/workbuddy-gateway) | 370 | 81 | 2026-10-25 | Go | 纯 Go 单二进制，**出站兼容文档质量最高**（`docs/outbound-compatibility.md` 逐项列头） |
| [NextAgentX/qoder-trae-workbuddy-switch](https://github.com/NextAgentX/qoder-trae-workbuddy-switch) | 227 | 53 | 2026-10-27 | Rust | WorkBuddy/TraeWork/Qoder 三线 × 双区 = 6 作用域；额度网关 + 统一网关（命名空间前缀）。**PolyForm Noncommercial 许可，禁止商用** |
| [yuan240324/workbuddy-openai-proxy](https://github.com/yuan240324/workbuddy-openai-proxy) | 43 | 18 | 2026-09-27 | JS | **零依赖本机版**（Node ≥18），代码量 4100 行，**可读性最好**，双区 + 号池 + 上下文压缩。文档 `docs/models-and-quota.md` 极详实 |
| [1416277987/proxy-hub](https://github.com/1416277987/proxy-hub) | 23 | 7 | 2026-08-21 | JS | 多平台适配器框架（CodeBuddy/TraeCN/TraeWork/Qoder），`adapters/` 一平台一文件，OCP 设计 |

**推荐参考顺序（给实现者）**：
1. `yuan240324/workbuddy-openai-proxy` — 单文件模块化，最快读懂全链路（登录→刷新→请求→额度）。
2. `linguo2625469/workbuddy2api-panel` — 错误分类/账号处置/风控对抗的**唯一权威实现**，`client.go` 的 `Classify` 注释极详细。
3. `CangShui/workbuddy-gateway` 的 `docs/outbound-compatibility.md` — 出站 header 逐项清单。
4. `aimod-cc/Agent2API` — 若要做「多平台额度型上游」的抽象层，它的 17 家提供商表格是现成设计参考。

---

## 9. 合规风险（必须写进设计文档）

### 9.1 官方协议原文（国内版）**【已核实】**

来源：《腾讯云代码助手软件许可及服务协议》，生效 2025-09-26，[workbuddy.cn/agreement](https://www.workbuddy.cn/agreement)

> **8.2 除非法律允许或腾讯云书面许可，您不得从事以下行为：**
> （2）对本软件进行**反向工程、反向汇编、反向编译**，或以其他方式尝试发现源代码。
> （4）对本软件运行过程中的**数据进行复制、修改、增加、删除、挂接运行**或创作任何衍生作品。
> （5）通过**修改或伪造指令、数据，变动软件的功能或运行效果**。
> （6）通过**非腾讯云授权的第三方软件、插件、外挂登录或使用本软件**。
> （7）自行或利用第三方软件对本软件及其组件、模块、数据等进行**干扰**。

> **2.3 腾讯云授予您一项个人的、不可转让及非排他性的许可**，以使用本软件。

> **8.4 腾讯云有权依合理判断对违反有关法律法规或本协议规定的行为进行处罚，包括警告、限制功能、删除屏蔽内容、封号、依法追究法律责任等。**

> **7.4** 若将本软件应用到产品中并对外提供服务，您作为**生成式人工智能服务提供者**，须遵守《生成式人工智能服务管理暂行办法》等，履行内容审核、标识、备案等义务。

**判定**：路线 B（逆向反代内置额度）**同时触碰 8.2 的 (2)(4)(5)(6)(7)**。把额度转成对外 API 还额外触发 2.3（许可仅限个人）与 7.4（对外提供服务需承担生成式 AI 服务提供者义务）。

### 9.2 官方文档里的合规红线（针对路线 A 也有约束）**【已核实】**

来源：[模型配置](https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model) 末尾

> 「若接入存在违法违规风险，依据用户协议 **§8.3.2**，WorkBuddy 可能**中止、限制或禁止该模型的接入**。」
> 「请勿将 API Key 分享给他人。」
> 「使用过程中可能持续触发模型调用，**建议密切关注您在第三方的账户费用**，避免出现超出预期的支出。」

→ 即使是合规的路线 A，**WorkBuddy 保留封禁某个自定义模型接入的权利**。AI Gateway 作为被接入方，需接受这个不确定性。

### 9.3 社区项目自己的合规声明（可作为措辞参考）**【已核实】**

- `workbuddy2api-panel`：「本项目仅限自用账号（签到/保活/个人工具接入），**不支持同时也是禁止**批量小号分发额度、二次打包或收费售卖」；「批量注册与转售接口配额**违反目标平台服务条款**」。
- `qoder-trae-workbuddy-switch`：**PolyForm Noncommercial 1.0.0**，「仅允许个人非商业使用，禁止任何形式的商业使用（含企业内部商业项目、SaaS/托管服务、售卖与捆绑分发）」。
- `Agent2API`：「以非官方客户端形态复用你自己账号的登录态，**可能不符合上游服务的使用条款**，风险（含账号被风控、封禁）自负；禁止商用或绕过计费」。

### 9.4 对 AI Gateway 的具体建议

| 做法 | 建议 | 理由 |
|---|---|---|
| 把 WorkBuddy 做成「平台类型」，用户填 token 直连其私有接口 | ❌ **不建议进主线** | 违反 8.2，且上游正在主动封堵，随时可能失效 |
| 把 WorkBuddy 做成「上游额度型平台」，仅在**用户本机**、**用户自己的账号**、**明确勾选免责**后启用 | ⚠️ 可选，但需强隔离 | 与社区项目定位一致；必须：不内置任何凭证、不上传凭证、不提供多账号池、不做额度转售 |
| 让 WorkBuddy 把 AI Gateway 当「自定义模型」供应商（路线 A） | ✅ **推荐** | 官方支持、零合规风险、能真正稳定长期运行 |
| 在 `/clients` 里增加 WorkBuddy 一键接入（写 `workbuddy/models.json`） | ✅ **推荐** | 与本项目现有 `/clients` 架构（Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes）完全同构，改动量小 |

> **注**：`workbuddy/models.json` 是**官方文档明文写出的路径**，写它是合规的；但该文件的结构官方未公开 JSON schema（只说「可视化界面配置」），**需要抓包或看客户端才能确定字段名——这是待验证项，不要编造。**

---

## 10. 网关集成要点（OpenAI 协议 / SSE / tools / reasoning）

### 10.1 作为**上游**（路线 B，AI Gateway 调 WorkBuddy）

| 能力 | 结论 | 依据 |
|---|---|---|
| OpenAI 协议兼容 | **上游不是 OpenAI 兼容**。是 `/v2/chat/completions` 的私有变体：请求体接近 OpenAI Chat，但**强制 stream**、role 白名单不含 `developer`、`tool_choice` 只收字符串、`image_url` 只收对象 | panel `payload.go`、yuan240324 |
| SSE 流式 | **上游只给 SSE**，非流式必须网关本地聚合 | panel、yuan240324 一致 |
| tools / function calling | **支持**，但有两个坑：① 流式 `tool_calls` 需按 `index` 合并；② DeepSeek 系的原生工具调用标记是**全角竖线的标记文本**，**仅在请求声明了 `tools` 时**才被上游解析成结构化 `tool_calls`；`tools` 在协议转换链中丢失时，标记会以纯文本落进 `content`，导致 Codex 这类只执行结构化 `function_call` 的客户端「回合直接结束、工具一次都没跑」 | panel「原生工具调用标记还原」 |
| reasoning 字段 | 上游用 `reasoning_content`（非 `reasoning`）。DeepSeek 思维链需**主动注入** `thinking.type=enabled`；多轮需 `reasoning_content` 回填到 assistant 消息；`reasoning_effort` 档位需按模型 `supportedEfforts` 降级 | panel `thinking.go` |
| OpenAI Responses | **上游没有 `/v2/responses`**（404 Route Not Found），必须网关侧 Chat↔Responses 双向转换 | CangShui `outbound-compatibility.md` |
| Anthropic Messages | **上游没有原生 Anthropic**，需转换。hub 明确警告：「`cache_control` 被忽略，长会话缓存命中率仅约 11%，**单次消耗超过 100 积分**」→ 默认禁用 | hub README |
| usage 字段 | 上游回 `usage.credit`（积分扣费）——**这是唯一的精确扣费来源**，必须采集 | yuan240324、panel |
| 缓存 token | 上游可能同时给 `prompt_tokens_details.cached_tokens` 与置 0 的兼容别名，需取最优值 | panel `usage.go` |

### 10.2 作为**下游**（路线 A，WorkBuddy 调 AI Gateway）

- AI Gateway 已暴露 OpenAI 兼容 `/v1/chat/completions` 与 Anthropic `/v1/messages`，WorkBuddy 的「自定义模型」可直接消费。
- **需要验证**：WorkBuddy 发过来的请求是否带 `stream: true`、是否带 `tools`、是否带 `reasoning_effort`。**【未核实，需抓包】**
- 建议在 AI Gateway 侧为该供应商准备一个「WorkBuddy 专用」后端配置：`param_overrides` 兜底 `stream_options.include_usage`，`reasoning_style` 按实际观察设定。

---

## 11. 公开资料不足 / 待验证清单

### 11.1 明确「公开资料不足」的项

| 问题 | 状态 | 已尝试的检索词 / 手段 |
|---|---|---|
| WorkBuddy 是否提供**面向第三方的公开模型 API** | **公开资料不足（倾向于「没有」）** | 抓取全站文档侧边栏（90+ 页）、腾讯云产品页、`workbuddy.cn/open`、`codebuddy.cn/open`、`copilot.tencent.com/open`（均无开放 API 入口）；Bing 检索「WorkBuddy 开放API 申请」「CodeBuddy 开放平台 API 文档」「WorkBuddy API key 官方」 |
| 积分「任务复杂度」系数的具体算法 | **公开资料不足** | 官方只说「与模型定价和任务复杂度两个因素有关」，未给公式 |
| 「积分耗尽」的**官方专属错误码** | **公开资料不足** | 官方错误码页只列 1001/3002/3003/3007/6003/6004/11115/11133/11134/14003，无余额类码；社区逆向补充 402 与 429+14018 |
| `workbuddy/models.json` 的 JSON schema | **公开资料不足** | 官方只说「配置参数仅保存在本地 workbuddy/models.json」，未公开字段结构 |
| WorkBuddy 自定义模型请求的**具体协议细节**（是否 SSE、是否带 tools） | **公开资料不足** | 官方文档只描述 UI 与「标准 /chat/completions 路径」 |
| 国际版是否也有「自定义模型」功能 | **已核实：有** | [intl 模型配置文档](https://www.workbuddy.ai/docs/zh/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model) 同样描述自定义模型 + 自定义协议开关 |
| 上游 `/v2/chat/completions` 的**官方契约** | **公开资料不足（本就不公开）** | 无任何官方文档；只有逆向项目源码 |
| 国内版 `hy4-preview` 与 `hy4-preview-f` 的区别 | **未核实** | hub 称 `-f` 是国际版试用变体（`modelId` vs `targetModelId`，trialDays=14），单一来源 |

### 11.2 如果确认要做路线 B，下一步怎么验证（具体抓包步骤）

1. **准备**：Windows 机器装官方 WorkBuddy 桌面端并登录（国内版 + 国际版各一个账号）；装 mitmproxy / Fiddler / Charles，安装根证书。
2. **定位 host**：过滤 `copilot.tencent.com`、`www.workbuddy.cn`、`www.workbuddy.ai`、`www.codebuddy.cn`。
3. **抓登录链路**：清空客户端登录态 → 重新登录，记录 `POST /v2/plugin/auth/state?platform=CLI` 的**完整请求头**（尤其 UA、X-CodeBuddy-Request、X-Device-Token、x-stainless-*）与响应。
4. **抓一次最小对话**：新建会话发「你好」，导出 `POST /v2/chat/completions` 的完整 request header + body + SSE response。**重点核对**：
   - UA 三段式的实际版本号（当前社区用 5.5.4 / 5.8.3，需确认最新）
   - 是否存在社区项目没覆盖的头（如 `x-stainless-*` 家族的具体值）
   - `X-Device-Token` 的真实来源与是否必需
   - body 里 `stream_options` / `thinking` / `verbosity` 的实际形态
5. **抓额度查询**：打开「设置 → 套餐与积分」，导出 `get-user-resource` 的 request body 与 response，核对 `ProductCode` 是否仍为 `p_taca`。
6. **抓工具调用**：在客户端里触发一次工具（如文件读取），对比 `tools` 声明存在 / 不存在时上游返回的 `tool_calls` 形态差异，验证「全角竖线标记」结论。
7. **抓自定义模型**（路线 A 的关键）：配一个本地假服务器当自定义模型，抓 WorkBuddy 发过来的**完整请求**，确定它是否 SSE、带哪些字段、是否带 tools。**这一步对设计路线 A 最有价值。**
8. **验证风控边界**：用抓到的最简头集，逐步删除非必要头，观察哪一项缺失会触发 11128 / 11140。

---

## 12. 给 AI Gateway 的设计建议（结论）

1. **主线做路线 A**：在 `/clients` 里新增 WorkBuddy 适配器，写 `workbuddy/models.json`，让 WorkBuddy 把 AI Gateway 当自定义模型供应商。这是官方支持、零合规风险、可长期维护的方向。**先抓包确定 `models.json` 的 schema，不要猜。**
2. **若要支持路线 B，必须做成「用户自担风险的旁路」**：
   - 不内置任何凭证，不提供账号池/批量注册，不做额度转售
   - 平台类型命名为「WorkBuddy（非官方，风险自担）」
   - 首次启用需弹窗展示 §9.1 的协议原文并显式勾选
   - 默认关闭，且不进入预设列表
3. **技术实现要点（若做路线 B）**：
   - 双 realm 独立配置（`cn` / `global`），不跨区混合轮询
   - 出站强制 `stream: true`，非流式本地聚合
   - 错误分类严格按 §6.5 的码表 + 文案双条件，**尤其 11128 vs 11140 必须按 code 分流**
   - 账号处置分级：402/14018 硬冷却到次日 04:00；429 软冷却指数退避；11128/11101/11115 不罚号
   - 采集 `usage.credit` 作为唯一精确积分扣费来源
   - refreshToken 反重放：单飞 + 持久化，避免并发刷新撞 12153
4. **文档里必须写的一句话**：WorkBuddy 官方**没有**面向第三方的模型 API，任何「WorkBuddy 平台」支持都是逆向私有接口，随时可能因上游风控而失效（已有 4561★ 项目因此停止维护）。

---

## 13. 关键源码定位（便于二次核对）

上文每条结论都已在正文附 URL，这里只给「想直接读实现」的入口：

| 想核对什么 | 文件 |
|---|---|
| 出站 header 全集、UA 三段式、会话头族 | `linguo2625469/workbuddy2api-panel` → `internal/upstream/headers.go` |
| 错误分类（`Classify`，码表与优先级注释最全） | 同上 → `internal/upstream/client.go` |
| 请求体改写管线（强制 stream、role/ tool_choice 归一） | 同上 → `internal/upstream/payload.go` |
| 积分余额查询与逐包构成 | 同上 → `internal/upstream/client.go`（`CreditPackages` / `enterpriseResource`） |
| 凭据文件结构与原子写回 | 同上 → `internal/auth/auth.go` |
| 登录 → 刷新 → 请求 → 额度 全链路（最易读） | `yuan240324/workbuddy-openai-proxy` → `src/{device-login,auth,headers,upstream}.mjs` |
| 站点表（双区 host / UA / 内置模型清单） | 同上 → `src/config.mjs` |
| 出站兼容逐项清单（含国际站与国内站差异） | `CangShui/workbuddy-gateway` → `docs/outbound-compatibility.md` |
| 桌面端凭据文件路径 | `1416277987/proxy-hub` → `adapters/codebuddy.js` |

---

*本文档所有「已核实」结论均附可点击来源；「未核实」项来自社区逆向项目，未经本机复现；「推测」项已显式标注。接口路径、字段名、域名一律照抄来源，未做任何推断性补全。*
