# Trae（字节跳动）模型接口 / 积分 / 合规 调研

> 2026-10-27。标注：**[已核实]** 有可复现来源；**[社区实测]** 第三方逆向源码/文档；**[未核实]** 无证据。
> 本会话 `web_search`/`web_fetch` 全 403，改用 `pwsh` + `Invoke-WebRequest`（GitHub raw / api.github.com / Bing / 官方 API 直连均通）。

## 0. 结论摘要

| # | 结论 | 置信度 |
|---|---|---|
| 1 | **无个人版公开模型 API**。唯一官方 API 是「企业版 OpenAPI」，只做人员/部门/用量/日志，**不含推理接口**。 | 已核实 |
| 2 | 官方"可编程"通道只有 **TRAE CLI 2.0 + ACP**（stdio，非 HTTP）。 | 已核实 |
| 3 | 社区逆向的是**私有协议**（`llm_raw_chat`/`llm_utils_chat`/`chat_sessions`），**不是 OpenAI 协议**。 | 社区实测 |
| 4 | 认证 `Cloud-IDE-JWT <token>`（国际版改 `Bearer`），来自 `storage.json` 的 `iCubeAuthInfo://icube.cloudide`；CN 自研 `tc` 加密（AES-128-CBC + 硬编码盐），INTL 部分版本明文。 | 社区实测 |
| 5 | 积分：**CN 按积分（token × 模型倍率）**，**INTL 按美元 usage** —— 两套体系，不能混。 | 已核实 |
| 6 | **没有 WorkBuddy 式的整体封堵**：主力仓库 2026-10 全在更新，无归档/停更。但**逐账号/逐 IP 风控是常态化产品能力**。 | 已核实+社区实测 |
| 7 | ToS 明确禁止「逆向工程」与「自动化访问/绕过限流」→ 反代**条款上属违约**。 | 已核实（原文） |

**交付建议**：P2 试验性平台，排在 WorkBuddy 之后。私有协议需持续跟版本、计费非 token（成本不可精确预估）、ToS 风险明确、个人账号部分端点不可用。

---

## 1. 官方 API 现状

### 1.1 有 API，但不是模型 API [已核实]
`https://docs.trae.cn/enterprise_trae-cn-enterprise-api?lang=zh` + `.../enterprise_authentication?lang=zh`

| 项 | 内容 |
|---|---|
| Base URL | `https://console.enterprise.trae.cn`（或企业专属域） |
| 前缀 | `/openapi/v1/` |
| 鉴权 | `POST /openapi/v1/auth/token`，body `{"app_id","app_secret"}` → `data.access_token` / `data.expire`(7200) / `token_type:"Bearer"` |
| 请求头 | `Authorization: Bearer {access_token}` |
| 限流 | 读 5 QPS / 写 3 QPS，超限 `HTTP 429` + `code:64290` + `Retry-After` |
| 能力 | 人员、部门、用量、数据分析、日志 —— **无 chat/completions**；仅旗舰版企业套餐 |

### 1.2 唯一官方可编程通道：TRAE CLI 2.0 + ACP [已核实]
`https://docs.trae.cn/cli_agent-client-protocol`：`traecli acp serve`，Zed ACP 协议，**stdio subprocess**；参数 `--permission-mode default|auto|bypass_permissions`、`-c 'model="<id>"'`、`-c 'model_reasoning_effort="high"'`。
环境变量（`.../cli_environment-variables`）：`TRAECLI_PERSONAL_ACCESS_TOKEN`（PAT）、`TRAECLI_HOST`、`TRAE_HOME`。
→ **合规但工程量大于反代**（需写 ACP↔OpenAI 适配层），建议作为合规备选写进设计文档。

### 1.3 个人版无公开 API [已核实]
`docs.trae.cn` sitemap 270 条 URL 无任何个人版 API/开发者页。个人版只有「自定义模型」（BYOK，把外部模型接进 Trae），方向相反 —— `.../ide_models` 的「添加自定义模型」只支持 OpenAI Chat Completions / Anthropic Messages 两种**出站**格式。

---

## 2. 社区逆向接口

### 2.1 四个可切换上游端点 [社区实测]
`autumnsentiment/Trae2api-cn` README（★81，2026-10-23）

| 端点 | 模式 | 方法 + 路径 | 说明 |
|---|---|---|---|
| Remote（默认） | `remote` | `POST {core}/api/remote/v1/chat_sessions` → `GET {core}/api/remote/v1/chat_sessions/{id}/events?reply_to_message_id={mid}` | 两步：建回合拿 `chat_session_id`+`message_id`，再拉 SSE |
| Solo | `solo` | `POST {code}/api/ide/v2/llm_raw_chat`（回退 `/api/ide/v1/...`） | `raw_chat_function=solo_agent` |
| IDE Agent | `ide` | 同上 | `raw_chat_function=chat_v3` / `builder_v3` |
| Work | `work` | `POST {core}/api/remote/v1/chat_sessions`，固定 `agent_type=agent_id=solo_work_remote` | TraeWork 产品线 |

第三条路径（另两项目）：`POST {core}/api/agent/v3/llm_utils_chat`，`function=solo_work_lite` + `config_name`（`wangqi233/trae2api` `src/trae-client.js`、`trae2api-hub` README）。**`llm_utils_chat` 无论 `stream:false` 都回 SSE**。
主机：CN `https://trae-api-cn.mchost.guru`（兜底 `api5-normal.mchost.guru`）；INTL `https://core-normal.traeapi.us`（兜底 `core-normal.trae.ai`、`coresg-normal.trae.ai`）。

### 2.2 `llm_utils_chat` 请求体 [社区实测]
```jsonc
{
  "messages": [{"role":"user","content":[{"type":"text","text":"..."}]}],  // content 必须 parts 数组，字符串→400
  "function": "solo_work_lite",       // 或 chat_v3 / inline_chat
  "stream": true,
  "config_name": "glm-5.2", "model": "glm-5.2",   // 同值，= 上游 model_name
  "temperature": 0.2, "top_p": 0.9, "max_tokens": 4096,
  "stop": ["..."], "presence_penalty": 0, "frequency_penalty": 0, "seed": 1, "n": 1
}
```
未在逆向记录中出现的字段一律不发，否则 `4001 param is invalid`。`llm_raw_chat` v2 是固定六字段：`config_name`、`conversation_id`、`messages`、`model_name`、`session_id`、`stream`。

### 2.3 `chat_sessions` 请求体 [社区实测]
`Trae2api-cn` `src/trae_remote_client.py`（真实代码）
```jsonc
{ "mode":"remote|work", "environment_id":"default", "env":"remote",
  "origin":"web", "auto_create_project":false,
  "initial_message": {
    "chat_session_id":"", "content":[],
    "query":"<扁平化全文>",                 // 消息被压成一个字符串
    "model_name":"glm-5.2",
    "agent_type":"solo_agent_remote",       // 或 solo_work_remote
    "agent_id":"solo_agent_remote",
    "model_selection_strategy":"auto|manual|max",
    "common_params":"<JSON 字符串>",         // user_id/device_id/machine_id/app_version/identity/product_code…
    "custom_model":{...}                    // strategy=manual 时
  }}
```
响应 `{"data":{"chat_session_id":"...","message_id":"..."}}`。
⚠️ **`query` 超约 50 万字符上游静默断流**（不报错直接结束流）；`TRAE_REMOTE_QUERY_MAX_CHARS` 默认 480000。

### 2.4 SSE 事件映射（接入网关最关键）[社区实测]
`XCool-603/trae2api` README「6. SSE 事件映射」+ `Trae2api-cn` `src/sse.py`

| 上游事件 | 字段 | → OpenAI |
|---|---|---|
| `output` | `response` | `delta.content` |
| `output` | `reasoning_content` | `delta.reasoning_content`（**推理字段名就是 `reasoning_content`**） |
| `output` | `tool_calls` | `delta.tool_calls`（按 `index` 聚合） |
| `token_usage` | 用量 | `usage` |
| `done` | `finish_reason` | `finish_reason` |
| `extra_info` | `content` = **完整思维链全文** | **必须丢弃** |
| `metadata`/`timing_cost`/`queue_*` | 排队元信息 | 丢弃（绝不能进 content） |

坑：`response` 与 `reasoning_content` 同帧互斥；SSE 含 `id:N` 行，不能只认 `event:`/`data:`；**上游发 `tool_calls` 时 `finish_reason` 仍是 `"stop"`，必须改写为 `"tool_calls"`**，否则客户端不执行工具。

### 2.5 tools / function calling [社区实测]
- `tools[].function.parameters` **必须是 JSON 字符串**，传 object → HTTP 400（`cannot unmarshal object into ... FunctionDefinition.tools.function.parameters of type string`）。
- 上游字段名 `function_call`（非 `tool_call`）；`name` 首帧给、`arguments` 后续帧流式续传，按 `index` 聚合。
- 通道差异：`Remote` 的 glm-5.3 工具测试 6/7（**不支持并行调用**）；`IDE Agent` 原生工具协议；`Remote`/`Work` 是**文本工具桥接**。
- 模型目录里 `custom_config.native_function_call` / `parallel_tool_calling` **按模型**下发。

### 2.6 是否 OpenAI 兼容
**不兼容**。私有协议 + 私有 SSE 事件名，网关必须做协议翻译。**SSE 流式可用**；**非流式需网关自行聚合**（`llm_utils_chat` 永远回 SSE）。

---

## 3. 认证

### 3.1 凭据形态 [社区实测，源码可验证]
| 项 | CN | INTL |
|---|---|---|
| Authorization | `Cloud-IDE-JWT <token>` | `Bearer <token>`（用 Cloud-IDE-JWT 被拒） |
| 同值冗余头 | `x-ide-token`、`X-Cloudide-Token` | `x-ide-token` |
| 身份/指纹头 | `x-uid`、`x-app-id`、`x-device-id`、`x-machine-id`、`x-ide-version(-code)`、`X-Trae-Client-Type:web`、`X-Preferenced-Language`、`x-user-region:CN`、`Origin:https://solo.trae.cn` | 同左 + `x-ide-version:3.6.2`、`x-app-version`、`store-country-code:us`、`Origin:https://solo.trae.ai` |

- **签到/额度类鉴权与 chat 不同**：只要 `Authorization: Cloud-IDE-JWT`，不需要全套指纹头；`GetUserInfo` 只认 `x-cloudide-token`（用 Authorization 会 401）。**混用会 401。**
- **`x-device-id` 有两套取值**：chat = `sha256(machineId)` 前 32 位；签到/额度 = 数字设备 ID（`icube-dc:` 后缀）。混用 401。

### 3.2 本地存储 [社区实测]
| 版本 | 路径 | 加密 |
|---|---|---|
| Trae CN | `%APPDATA%\Trae CN\User\globalStorage\storage.json` | `tc` |
| TRAE SOLO CN | `%APPDATA%\TRAE SOLO CN\User\...` | `tc` |
| Trae 国际版 | `%APPDATA%\Trae\User\...` | **明文 JSON** |
| TRAE SOLO 国际版 | `%APPDATA%\TRAE SOLO\User\...` | `tc` |

键：`iCubeAuthInfo://icube.cloudide`（凭据）、`iCubeAuthInfo://icube-dc`（设备 ID）、`telemetry.machineId`。模型目录在同目录 `state.vscdb`（SQLite）的 `ItemTable` → `<uid>_AI.agent.model.model_list_map`。

### 3.3 `tc` 加密（"密钥从哪来"的答案）[社区实测，两项目独立实现一致]
`laojichao/trae-local-api` README + `Trae2api-cn` `src/trae_decrypt.py`
```
Base64 → [6B Header][32B RandomBytes][N EncryptedData]
Header 0x74 0x63 ("tc") + 0x05 0x10 0x00 0x00 → AES
key/iv = SHA-512( SHA-512(RandomBytes) XOR salt64 ) → 前16B=Key, 次16B=IV
AES-128-CBC → [64B SHA-512 Hash][PKCS7 padded plaintext JSON]
盐 SALT_A^SALT_B（AES）/ SALT_C^SALT_D（AES_PRIVATE），硬编码在客户端前端 JS
```
→ **不需要 keychain/DPAPI，盐硬编码在 bundle 里，密钥材料全来自文件自身**。所以"拷走 storage.json 即可离线解密"成立（`XCool-603` 的"便携快照"即此）。

### 3.4 有效期与 refresh [社区实测]
- 字段：`token`、`refreshToken`、`expiredAt`/`TokenExpireAt`、`refreshExpiredAt`、`clientID`、`userId`、`host`。
- 刷新：`POST {account_host}/cloudide/api/v3/trae/oauth/ExchangeToken`，body `{"ClientID","RefreshToken","ClientSecret":"-"}` → `Token`/`RefreshToken`/`TokenExpireAt`。
- ⚠️ **刷新会轮换整个令牌族**：`wangqi233/trae2api` README 明确「会让同一账号在其它客户端里的登录静默失效」，故其 `TRAE_POOL_SELF_RENEW` **默认 off**。**接入设计必须决策这一项。**
- 临时 token：`POST {account_host}/cloudide/api/v3/trae/GenerateTempToken`（社区记 7 天，仅排障）。token 是 JWT，payload `data.id` 即 userId。

---

## 4. 域名对照表

| 域名 | 角色 | 等级 |
|---|---|---|
| `www.trae.cn` | 国内站官网（`trae.com.cn` 301 到此） | 已核实（HTTP 实测） |
| `www.trae.ai` | 国际站官网 | 已核实 |
| `docs.trae.cn` / `docs.trae.ai` | 文档站（后者是 SPA，正文抓不到） | 已核实 |
| `api.trae.cn` | **CN 成长/商业面**：`/trae/api/v2/ug/checkin_credits/*`、`/trae/api/v2/pay/ide_user_ent_usage`、`/cloudide/api/v3/trae/GetUserInfo`、`/cloudide/api/v3/trae/terms/get` | 已核实（本次直连 POST 通） |
| `api.trae.com.cn` | CN 账号/OAuth/pay 备用；`/api/ide/v1/*` 打过去 **404** | 社区实测 |
| `trae-api-cn.mchost.guru` | **CN 模型数据面**（`/api/ide/*`、`/api/agent/*`、`/api/remote/*`） | 社区实测（多项目一致） |
| `api5-normal.mchost.guru` | CN 备用调度 / TraeWork 商业用量 | 社区实测 |
| `core-normal.trae.ai` / `coresg-normal.trae.ai` | INTL 模型数据面（美/新） | 已核实（trae.ai bundle 常量）+ 社区实测 |
| `core-normal.traeapi.us` / `grow-normal.traeapi.us` | INTL USTTP 账号的数据面 / 账号面 | 社区实测 |
| `grow-normal.trae.ai` | INTL 账号面（OAuth） | 已核实（bundle）+ 社区实测 |
| `ug-normal.trae.ai` / `ug-normal.us.trae.ai` | INTL 成长/权益（`terms/get` 在此返英文 ToS） | 已核实（本次实测） |
| `icube-normal.trae.ai` | INTL 登录/版本检查 | 已核实（bundle） |
| `console.enterprise.trae.cn` | 企业版 OpenAPI base | 已核实（官方文档） |

**基址分离陷阱**：`/api/remote/v1` 只在 Remote 通道用；IDE/Solo 路由 rooted 在同一主机根，混用生成 `/api/remote/v1/api/ide/...` → 404/502（`Trae2api-cn` issue #9）。

---

## 5. 模型清单

### 5.1 官方内置（CN，20 个）[已核实]
`.../ide_models`：Seed-Evolving（会员专属）、Seed-2.1-Pro-0915、Seed-2.1-Turbo、Seed-Code、MiMo-V2.6-Flash/Pro、Step-5-Preview、GLM-5.3-FlashX/Flash/5.3/5.2、DeepSeek-V4.1-Flash、DeepSeek-V4-Flash/Pro 正式版、Kimi-K3、Kimi-K2.8-Preview、MiniMax-M3、Qwen3.8-Flash/Max、Qwen3.7-Plus。

### 5.2 客户端真实目录（含 upstream key）[社区实测]
`trae2api-hub` 从 `state.vscdb` 导出（2026-10-17，raw.githubusercontent.com 可读）。

**CN chat 面 20 个**（展示名→upstream key→积分倍率）：`Seed-Code`→`Doubao-Seed-Code`→0.06；`Seed-2.1-Pro-0915`→`Doubao-Seed-2.1-Pro`→0.8；`Seed-2.1-Turbo`→`Doubao-Seed-2.1-Turbo`→0.2；`DeepSeek-V4.1-Flash`→`deepseek-v4.1-flash`→0.06；`DeepSeek-V4-Flash`→`DeepSeek-V4-Flash-Official`→0.2；`DeepSeek-V4-Pro`→`DeepSeek-V4-Pro-Official`→0.72；`GLM-5.3`→`glm-5.3`→0.78；`GLM-5.3-Flash`→`glm-5.3-flash`→0.06；`GLM-5.3-FlashX`→`glm-5.3-flashx`→0.31；`GLM-5.2`→`glm-5.2`→0.78；`Kimi-K3`→`kimi-k3`→1.83；`Kimi-K2.8-Preview`→`kimi-k2.8-preview`→0.98；`MiMo-V2.6-Pro/Flash`→`mimo-v2.6-pro`/`-flash`→0.15/0.06；`MiniMax-M3`→`minimax-m3`→0.26；`Qwen3.8-Max`→`qwen3.8-max`→1.5；`Qwen3.8-Flash`→`qwen3.8-flash`→0.08；`Qwen3.7-Plus`→`qwen-3.7-plus`→0.25；`Step-5-Preview`→`step-5-preview`→0.48。

**INTL 10 个**：`Gemini-2.5-Pro`→`gemini-2.5-pro-latest`；`Gemini-2.5-Flash`→`gemini_2.5_flash_premium`；`Gemini-3-Flash-Preview`→`gemini_3_flash`；`Gemini-3.1-Pro-Preview`→`gemini_3.1_pro`；`Claude-Opus-4.7`→`aws_claude_4.7_opus`；`Claude-Sonnet-4.6`→`aws_claude_4.6_sonnet`；`GPT-5.5`→`gpt-5.5`；`GPT-6-Astra`→`gpt-6-astra`；`Grok-4`→`grok-4`；`Kimi-K2-0905`→`kimi-k2`。

### 5.3 两区差异（关键）
- **模型名两区几乎不通用**：各 47 个名只有 3 个交集（`glm-5.2`、`minimax-m3`、内部 `summary`）。用错区 → `4001 param is invalid`。
- **CN = 豆包/DeepSeek/GLM/Qwen/Kimi 系；INTL = Claude/GPT/Gemini/Grok 系** —— 两套池不重叠，这就是"国内版 vs 海外版"的核心差异。
- **UI 展示名 ≠ config_name**：INTL 的 `Seed-2.1-Turbo` 实际 key 是 `Dola-Seed-2.0-Code`；直呼 UI 名 4001。
- 命名形态：第一方裸名（`glm-5.3`）；第三方 BYOK 槽 `<渠道>//<模型名>`（`Kimi-CN//kimi-k3`），**裸 `llm_utils_chat` 不认**（4001/4023）。
- 目录**按账号下发**：`get_detail_param`/`model_list` 返回的才是可调集合。

---

## 6. 积分机制（官方口径）

### 6.1 两套计费体系 [已核实]
| | 国内版 trae.cn | 国际版 trae.ai |
|---|---|---|
| 单位 | **积分（credits）** | **美元 usage**（"$20 usage / month"） |
| 定价页 | `https://www.trae.cn/pricing` | `https://www.trae.ai/pricing` |
| 档位 | 免费 / Lite ¥49 / Pro ¥99 / Pro+ ¥239 / Ultra ¥699 | Free / Pro $20 / Pro+ $60 / Ultra $200 |
| 月度额度 | 500 / 2,000 / 4,000 / 12,000 / 40,000 积分 | — / $20 / $60 / $200 usage |
| 计量 | **token × 模型"积分消耗速度"倍率**（非按次） | usage 折算（公式未公开） |

INTL pricing 页 `loaderData`：`basic_usage_limit: 1/20/60/200`、`credits_limit: 0` —— **国际版压根不用积分字段**。

### 6.2 积分定义与获取 [已核实]
`.../ide_plans-and-billing`
- 类型：**会员积分**（订阅/增购）+ **奖励积分**（活动）。
- 免费用户：每月登录 500（当月有效）、注册一次性 2000（31 天）、**每日签到 100**（31 天）、邀请各 500。
- 会员：套餐积分（31 天）+ 每月登录 500 + **每日签到 200** + 邀请 500。
- **只有内置模型消耗积分；自定义模型（BYOK）不消耗。**
- 消耗顺序：先烧剩余有效期最短的 → 同有效期奖励积分优先于会员积分。
- 增购 ¥50/1000 积分（31 天）；退还仅限"底层模型报错或死循环"。
- **积分是 31 天有效期的钱包，不是每日/每月刷新的配额** —— 最易误解点。

### 6.3 消耗倍率 [已核实]
`.../ide_limited-time-discount-for-builtin-models`（忙时 8:00–22:00，闲时 22:00–次日 8:00 自动打折）：DeepSeek-V4-Pro 忙时 0.72（闲时 0.36）；DeepSeek-V4-Flash 0.20（0.10）；DeepSeek-V4.1-Flash 0.12（免费闲时 0.06 / 会员全时段 0.06）；GLM-5.3 0.78（会员限时 0.39）；Seed-2.1-Turbo 0.4（会员 0.1）；Seed-Code 0.12（会员 0.03）。
机器可读版本：目录里 `features.consumption_rate.data.rate` 与 `features.activity_discount`。

### 6.4 额度查询接口 [社区实测]
| 用途 | 方法 + 路径 | 关键字段 |
|---|---|---|
| 权益包/剩余积分 | `POST https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage`，body `{"require_usage":true,"req_source":0}` | `user_entitlement_pack_list[]` → `entitlement_base_info.quota.credits_limit`（-1=无限）、`usage.credits_amount`、`user_pay_identity(_str)`、`quota.solo_agent_parallel_limit` |
| 付费状态 | `POST /trae/api/v1/pay/ide_user_pay_status` | — |
| 签到状态 | `POST /trae/api/v2/ug/checkin_credits/status`，body `{"req_source":1}` | `{"checked_in":true,"code":0,"credits":100,"enable":true,"extra_credits":100,"message":"success"}`（CN 实测 2026-10-19） |
| 领取签到 | `POST /trae/api/v2/ug/checkin_credits/claim` | 同上 |
| 单回合用量 | `POST /api/v1/commercial/get_session_usage`，body `{"session_id":"<userMessageId>"}` | `data.user_usage_group_by_session` → `credits_float`/`credits_consumed` |
| 模型目录 | `POST /api/ide/v1/model_list`，body `{"type":"chat"}` | **CN 可用；INTL 返回 403 no schema found** |

`req_source`：1=通用 IDE 积分，2=Work 专属。**CN 的通用积分与 Work 积分是两个钱包**，通用用尽后只有走 Work 端点才能用 Work 专属积分（`Trae2api-cn` issue #3）。

### 6.5 错误码
官方（`.../ide_error-codes`）[已核实]：`1001/1002` 认证失效；`4011` 今日用量已达上限；`4013` 地区不可用；**`4015` 帐户/IP 风险请求被阻止**；**`4010` 检测到风险账号已自动登出**；`4021` 会话次数上限；`4022` 上下文超限；`4023` 模型列表已更新；`4028` 第三方厂商原始报错；`4001` 请求失败（官方兜底文案）；`3004/4007` 限流。
社区补充 [社区实测]：**`4008`** 账号配额耗尽；**`1005`** 套餐权益不足（模型级）；**`4017`** 设备/IP 级风控；**`9074`** 签到风控码；**`2001`** `failed to get app config: record not found`（个人账号调 `/api/ide/v2/llm_raw_chat` 固定失败）；`4019` INTL 风控拒绝。

---

## 7. 开源反代项目（2026-10-27 实测）

来源 `https://api.github.com/search/repositories?q=trae2api&sort=stars`（可复现）。

| 仓库 | ★ | 最近提交 | 语言 | 说明 |
|---|---|---|---|---|
| [NextAgentX/qoder-trae-workbuddy-switch](https://github.com/NextAgentX/qoder-trae-workbuddy-switch) | 227 | 2026-10-27 | Rust/Tauri | Trae+WorkBuddy+Qoder 六作用域 + 网关（`:57894` Trae），PolyForm 非商用 |
| [autumnsentiment/Trae2api-cn](https://github.com/autumnsentiment/Trae2api-cn) | 81 | 2026-10-23 | Python | 资料最全：4 端点、Responses/Anthropic、签到、积分流水、1M Max |
| [JeffHu0912/trae2api](https://github.com/JeffHu0912/trae2api) | 53 | 2026-09-08 | Go | SOLO 反代 + 多账号面板 |
| [connectedGraph/trae2api-web](https://github.com/connectedGraph/trae2api-web) | 49 | 2026-10-11 | Go | SOLO 通道 + Web 控制台 |
| [linqiu919/trae2api](https://github.com/linqiu919/trae2api) | 39 | 2025-06-27 | — | **已 archived**（唯一停更，早于本轮生态） |
| [shuishuipingan/trae2api-hub](https://github.com/shuishuipingan/trae2api-hub) | 19 | 2026-10-23 | Python | **双区 CN/INTL** + 模型目录快照 + 额度预警 |
| [wangqi233/trae2api](https://github.com/wangqi233/trae2api) | 10 | 2026-09-20 | Node | **`docs/PROTOCOL.md` 最有价值的协议实测记录** |
| [XCool-603/trae2api](https://github.com/XCool-603/trae2api) | 6 | 2026-10-27 | Go | README「协议要点/坑」质量最高 |
| [laojichao/trae-local-api](https://github.com/laojichao/trae-local-api) | 56 | 2026-07-24 | Node | `tc` 解密 + 四版本适配 |

### 7.1 公认的坑（按代价排序）
1. **UA 被用于机器人识别**（最坑）：Node 默认 UA ✅、官方客户端 UA ✅、`axios/1.7.7` ✅，但 **Go 默认 `Go-http-client/1.1` → HTTP 200 + 业务码 4011「超出限流」**。错误信息完全指向"限流"，与真实原因无关。
2. **`x-device-id` 两套取值**（chat vs 签到/额度），混用 401。
3. **WAF IP 级封禁**：60s 内 ≥2 个**不同** UID 吃 403 → 判 IP 被封，应立即停轮转（继续换号只会放大请求量、延长封禁）。
4. `extra_info.content` 是完整思维链全文，当正文累加会污染回复。
5. `finish_reason` 不改写就丢工具调用。
6. `tools[].function.parameters` 必须 JSON 字符串。
7. `content` 必须 parts 数组，字符串直接 400。
8. `query` > 约 50 万字符上游静默断流。
9. 签到 `9074` 需轮换设备 ID + 指数退避，立刻重试会延长限流窗口。
10. 个人账号 `llm_raw_chat` v2 固定 `2001`（缺企业 app config）。
11. **刷新 token 会踢掉同账号其它客户端登录**。
12. 并发槽位按账号 entitlement（`solo_agent_parallel_limit`），免费账号保守 2 槽。

---

## 8. 直接回答：Trae 有没有"封堵迹象"？

### 8.1 结论：**没有 WorkBuddy 式的整体封堵**
| 判据 | 观察 |
|---|---|
| 仓库存活 | 17 个 `trae2api` 仓库，**仅 1 个 archived**（2025-06 老项目，早于本轮生态） |
| 更新活跃 | 前 6 名 2026-10 均有提交；`XCool-603` 2026-10-27（当天）、`Trae2api-cn`/`hub` 10-23 |
| 停更声明 | 三份主要 README 关键词扫描：封堵 0、停更 0、不再维护 0 |
| Issue 内容 | `Trae2api-cn` 13 个 issue 全是功能/部署（Docker、端点切换、工具调用、签到），**无一条"上游封杀"** |
| 新增能力 | 2026-10 仍在加：1M Max、`/v1/responses`、Anthropic Messages、签到自动化、WAF fail-fast |

### 8.2 但风控是**产品化、持续**的
| 证据 | 性质 |
|---|---|
| 官方错误码表含 `4010`（风险账号自动登出）、`4015`（帐户/IP 风险阻止请求） | **官方承认并产品化账号/IP 风控** |
| `4017` 设备/IP 级风控（社区实测） | 设备指纹被校验 |
| UA 被用于机器人识别（Go UA → 4011） | 客户端指纹校验 |
| WAF IP 级封禁（多账号同窗口 403） | IP 层封禁 |
| 签到 `9074` 风控码 | 签到接口独立风控 |
| Issue #15「调用时桌面端也会有消息提示」+ 作者回复「web/客户端/移动端全是一套 web 会话调用逻辑，会话全是同一个记录」 | **反代会话在官方客户端可见 → 可被发现** |
| `2001` app config：个人账号 raw v2 被权限卡死 | 端点级权限收窄的先例 |
| 4011 语义漂移（官方=用量上限，社区实测 UA 差→限流） | 风控与限额混在同一码，**难区分** |

### 8.3 交付建议
风险不是"某天一刀切"，而是**持续性逐账号/逐 IP 封禁**。这比整体禁用好处理（账号池 + 出口隔离可缓解），但网关必须具备：
1. **错误码分类器**：`4010/4015/4017`（风控，不重试、换号）、`4008`（配额，冷却账号）、`1005`（权益，换模型）、`4001/4023`（参数/目录，不重试）、`3004/4007`（限流，退避）分开处理 —— 混在一起重试会放大封禁。
2. **IP 级 fail-fast**：同出口 60s 内 ≥2 个不同账号 403 → 停轮转。
3. **UA / 设备指纹固定**：伪装官方客户端（node-fetch UA + 稳定 device/machine id），**每账号一套指纹**。
4. **可选出口代理按账号隔离**。

**结论：P2（WorkBuddy 之后），定位"实验性平台 + 强风控处理"；不建议作为首发主力。**

---

## 9. 合规风险（ToS 原文）

### 9.1 国内版《用户协议》[已核实]
取法：`POST https://api.trae.com.cn/cloudide/api/v3/trae/terms/get?Language=CN&TermsType=service_terms` → `{"Result":{"Title":"[Trae]用户协议","SubTitle":"更新时间: 2026/06/05","Content":"..."}}`（`api.trae.cn` 同接口返回 GBK 乱码，用 `.com.cn` 拿 UTF-8）。官网 `/terms-of-service` 是 SPA，正文靠此接口渲染。

- **5.2**「你不得自行或允许、协助任何人使用插件、外挂、系统或第三方工具干扰、破坏、修改本服务的正常运行或施加其他影响，**包括但不限于使用任何自动化程序、软件或类似工具接入本服务**，收集或处理其中信息、内容；」
- **5.3(9)**「对本服务进行**逆向工程、拆卸、反向汇编、反编译、编译**，或以其他方式试图发现本服务或第三方服务的源代码、系统的源代码或底层组件、底层技术、算法、模型；」
- **5.3(11)**「**出租、租赁、分发、许可、转让或出售**、改编、复制、传播、垂直搜索、镜像或交易等访问或使用本服务或其任何部分；」
- **5.3(14)**「**以自动化的方式使用本服务**或通过本服务访问第三方服务，尤其是可能对我们的基础设施造成不合理或不成比例负载的行动、或**超出我们的任何速率限制**；」
- **5.3(15)**「使用任何"深度链接"、"页面抓取"、"蜘蛛"或其他**自动设备、程序、算法或方法**……访问、获取、复制或监控本服务的任何部分……」
- **5.3(16)**「……或**绕过我们可能用于防止或限制访问本服务的任何措施**……」
- **5.4**「……我们可独立进行判断并有权不事先通知你，自行采取处置措施，处置措施包括但不限于**警示、限制功能、暂停或终止向你提供服务**，并追究你的相关法律责任。」
- **4.4**「你注册的账号仅限于你本人使用，未经公司书面同意，**禁止以任何形式赠与、借用、出租、转让、售卖或以其他方式许可他人使用该账号**。」

### 9.2 国际版 ToS [已核实]
取法：`POST https://ug-normal.trae.ai/cloudide/api/v3/trae/terms/get?Language=EN&TermsType=service_terms` → `Title:"TRAE Terms of Service"`, `Last Updated: September 17, 2026`。页面入口 `https://www.trae.ai/terms-of-service`。

**Section 5. Prohibited Uses**：
- 「**autonomously or programmatically extract output**, or take any action that imposes or may impose an unreasonable or disproportionately large load on our infrastructure, or **rent, lease, distribute, license, transfer, or sell the Services or any part thereof**;」
- 「**reverse engineer, disassemble, decompile** or otherwise attempt to discover the source code or underlying technologies, algorithms, or models of the Services; or engage in hacking, password "mining", or any other illegitimate means, or disrupt, damage, probe, scan, or test the vulnerability of the Services;」
- 「develop artificial intelligence or machine learning models or any products or services in direct competition with TRAE…」
- 执法：「…whether by automated or non-automated means. **We may suspend or terminate your use of the Services**…」；管辖：Singapore law + SIAC 仲裁（Section 17）。

### 9.3 定性
| 行为 | 条款定性 |
|---|---|
| 读本地 `storage.json` 解密凭据 | 5.3(9) 逆向工程 → **违约** |
| 个人账号 token 直连私有接口跑第三方客户端 | 5.2 + 5.3(14) → **违约** |
| 把额度转成 OpenAI API 供他工具/他人使用 | 5.3(11) + 4.4 → **明确违约** |
| 多账号池、签到自动化 | 5.3(10) + 5.3(14) → **违约** |
| TRAE CLI 2.0 + ACP 做本机 agent 集成 | 官方文档支持 → **合规**（推荐） |
| 企业版 OpenAPI | 官方文档支持 → **合规** |

→ **Trae 反代在 ToS 上无灰色空间**，风险高于一般"未公开接口"。开源项目均写"仅供学习研究"，`NextAgentX` 直接用 PolyForm Noncommercial 并明写「不得转售、转租、共享额度」。

---

## 10. 对 AI Gateway 的接入建议

| 项 | 建议 |
|---|---|
| 协议适配 | 需新增 `src/protocol/trae` 适配器；**不能走同协议直通**，必须重建 `UnifiedResponse` |
| 流式 | SSE 可用；处理 `output{response|reasoning_content|tool_calls}`、`token_usage`、`done`，丢弃 `extra_info`/`metadata`/`queue_*` |
| reasoning | 出站 `reasoning_effort` → `light`/`high`/`extra_high`（官方称"轻/高/极高"）；入站统一到 `reasoning_content` |
| tools | 支持；`parameters` 必须序列化为 JSON 字符串；`finish_reason` 需改写；并行调用按通道/模型能力裁剪 |
| 计费 | **不能按 token 计价**：CN 是"积分 = token × 倍率"，INTL 是美元 usage。建议 `cost` 置空或单列 `credits`，避免与 `models` 表 token 单价模型冲突 |
| 认证 | ① 读客户端 `storage.json`（`tc` 解密）② 网页 OAuth（`https://www.trae.cn/authorization`，回调必须 `http://127.0.0.1:<port>/authorize`） |
| token 刷新 | **默认关闭自动刷新**（会踢掉用户客户端登录），或明确提示二选一 |
| 平台类型 | 新增 `PlatformType::Trae` / `TraeIntl`，`catalog_key` 必须含 realm（模型名两区不通用） |
| 错误映射 | `4010/4015/4017`→不重试+账号冷却；`4008`→配额；`1005`→换模型；`4001/4023`→参数错不重试；`3004/4007`→退避 |
| 合规 | UI 需明确提示"非官方接口，可能违反服务条款"；不建议默认开启 |

---

## 11. 未核实 / 公开资料不足

1. **token 具体有效期**：`expiredAt` 存在但无项目公开实测值，官方无文档。
2. **INTL 的 usage→美元 折算公式**：定价页 FAQ 答案由前端 JS 渲染抓不到；`docs.trae.ai` 全站 SPA（正文仅 96 字节）。
3. **国内版是否有"每日/每月重置的免费配额"**：官方只写"积分 31 天有效期"，`4011` 提"今日用量已达上限"但无阈值。**两者可能并存（钱包 + 日限额），未核实。**
4. **`4001` 官方语义（"请稍后重试"）vs 社区实测（`param is invalid`）冲突**：疑为官方兜底文案。
5. **是否有官方签名/请求体加密**：`wangqi233` 提到 `encrypted_prompt_set` 与"云端 agent 通道需完整 `model_info`（`ak`/`base_url`/`custom_model_type`）"，说明**部分高级通道有加密参数**；chat 主链路未见签名，**是否所有端点都无签名未核实**。
6. **embeddings / 图片生成**：trae.ai bundle 里有 `text_to_image`/`tool_text_to_image` 端点，无项目实现，**可用性未核实**。
7. **WorkBuddy"上游已禁用反代"的具体公告**：未独立核实（Lead 提供），Trae 侧未找到对应物。

**已试检索词**：`Trae 官方 API 文档 开放平台`、`trae.ai official API documentation developers`、`site:trae.ai terms of use`、`Trae 用户协议 禁止 自动化 第三方 调用`、`trae2api`、`trae api proxy`、`Trae 逆向`、`Trae 无限积分`、`trae openai compatible`、`Trae SOLO api`、`trae 9074`、`trae 4015`。
**抓取失败**：`docs.trae.ai`（SPA）、`www.trae.cn/terms-of-service`（SPA，已用 `terms/get` 接口绕过）、`web.archive.org`（429）、`r.jina.ai`（403）。

---

## 12. 来源清单

**官方（已核实）**
`docs.trae.cn` 的 `enterprise_trae-cn-enterprise-api` / `enterprise_authentication` / `ide_models` / `ide_plans-and-billing` / `ide_limited-time-discount-for-builtin-models` / `ide_error-codes` / `ide_model-thinking-effort` / `ide_device-limit` / `cli_agent-client-protocol` / `cli_environment-variables`；`www.trae.cn/pricing`；`www.trae.ai/pricing`；`POST api.trae.com.cn/cloudide/api/v3/trae/terms/get`（CN 协议 2026/06/05）；`POST ug-normal.trae.ai/cloudide/api/v3/trae/terms/get`（INTL ToS 2026-09-17）。

**社区（逆向，非官方）**
`github.com/wangqi233/trae2api/blob/main/docs/PROTOCOL.md`；`github.com/autumnsentiment/Trae2api-cn`（README 四端点表 + `src/trae_remote_client.py`、`trae_client.py`、`trae_decrypt.py`、`sse.py`、`.env.example`）；`github.com/shuishuipingan/trae2api-hub`（`trae_client.py` + `trae_catalog*.json`）；`github.com/XCool-603/trae2api`（UA 坑 / WAF / SSE 映射 / 已知限制）；`github.com/laojichao/trae-local-api`（`tc` 四版本对照）；`github.com/connectedGraph/trae2api-web`；`github.com/NextAgentX/qoder-trae-workbuddy-switch`。
