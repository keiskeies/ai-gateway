# 「快捷连接」API Key 申请页调研

> 调研对象：AI Gateway 网关 UI 中的「快捷连接」功能——一键跳转到各家 API Key 申请页面。
> 用户原话：「还要有快捷连接访问到魔塔社区 api-key 申请地址, 以及 agens 海内外 api-key 申请地址」
>
> **核实方式**：本会话的 `web_search` / `web_fetch` 工具全部后端 403 失败，因此所有 URL 均由 `pwsh`（`Invoke-WebRequest` / `curl.exe`）实际发起 HTTP 请求验证。状态码为实测值。
> **核实时间**：2026-10-27（UTC+8）
> **证据分级**：`[实测]` = 本次真实发起 HTTP 请求得到的状态码/内容；`[文档]` = 从官方文档正文读到；`[推测]` = 由间接证据推断，未直接验证。

---

## 0. 结论速览（给 Lead 的三句话）

1. **「魔塔社区」= 魔搭社区 ModelScope**（modelscope.cn，阿里达摩院）。API Key 页面**已改版**：历史地址 `/my/myaccesstoken` 现在**重定向到 `/my/settings/token`**，快捷连接应直接指向新地址。
2. **「agens」= Agnes AI**（拼写 a-g-n-e-s）。仓库代码里的 `agnes-3.0-flash`、`agnes-5`、`src/protocol/openai.rs:437` 注释可作交叉印证。它确实是**双站**：海外站 `agnes-ai.com` / 国内站 `agnes-ai.cn`，**两站的 API base_url 不同**，网关需要做「双区」平台配置。
3. 两家的 Key 申请页都**不需要先登录就能打开**（未登录会渲染登录页），可以安全地作为外链 `target="_blank"` 使用。

---

## 1. 魔搭社区 ModelScope

### 1.1 称呼对应关系

| 用户说法 | 实际名称 | 说明 |
| --- | --- | --- |
| 魔塔社区 | **魔搭社区 ModelScope** | 用户口语常把「魔搭」写成「魔塔」。官方中文名「魔搭社区」，英文品牌名 ModelScope，简称「魔搭」。依据：`https://www.modelscope.cn/llms.txt` 原文——「"ModelScope"是官方英文品牌名，"魔搭社区"是中文名，"魔搭"是常见简称，**不要使用其他变体**」`[文档]` |

### 1.2 URL 可达性实测

| URL | 用途 | 可达性（实测） | 备注 |
| --- | --- | --- | --- |
| `https://www.modelscope.cn/my/settings/token` | **API Key / Access Token 申请页（新地址，推荐）** | **200** | `[实测]` SPA 页面，title 恒为 `ModelScope`。**这是当前正确地址** |
| `https://www.modelscope.cn/my/myaccesstoken` | 历史地址 | **200**（HTTP 层） | `[实测]` + `[文档]` 前端路由表里该条已写成 `{path:"/my/myaccesstoken", exact:!0, **redirect:"/my/settings/token"**}`，即**浏览器里会被前端重定向**到新地址。HTTP 层仍是 200 是因为 SPA 对 `/my/*` 一律返回外壳 |
| `https://www.modelscope.cn/my/access/token` | 历史地址变体 | **200**（HTTP 层） | `[实测]` 同样 `redirect:"/my/settings/token"` |
| `https://www.modelscope.cn/docs` | 文档中心索引 | **200** | `[实测]` title=`文档中心 - ModelScope 魔搭社区` |
| `https://www.modelscope.cn/docs/model-service/API-Inference/intro` | **API-Inference 推理服务文档（如何调用）** | **200** | `[实测]` title=`API推理介绍` |
| `https://www.modelscope.cn/docs/model-service/API-Inference/limits` | **API-Inference 使用限制/免费额度** | **200** | `[实测]` title=`API-Inference使用限制` |
| `https://modelscope.ai/docs/model-service/API-Inference/limits` | 国际站同页（英文） | **200** | `[实测]` 海外站 `modelscope.ai` 与 `.cn` 内容同步 |
| `https://api-inference.modelscope.cn/v1` | **推理 API base_url** | **401**（`/v1/models` 无鉴权时 401） | `[实测]` 401 = 端点存在且要求鉴权，非 404 |
| `https://api-inference.modelscope.cn/v1/models` | 模型列表 | **200**（**无需鉴权**） | `[实测]` 返回 37 个模型 id，OpenAI `{"object":"list","data":[...]}` 结构 |

> ⚠️ **踩坑提示（重要）**：ModelScope 站点是 umi SPA，**任意 `/my/xxx` 路径都返回 HTTP 200**。因此「200」不能证明路径存在。上表中 `myaccesstoken` / `access/token` 的结论来自前端路由表（`umi.js` 里的 `redirect` 字段），`my/settings/token` 来自同一张路由表的 `path` 字段——这是当前唯一有效的判断依据。文档页 `/docs/**` 同理：`.md` 后缀对 `/docs/**` **不生效**（仍返回 SPA 外壳），只对 `/models/**`、`/datasets/**` 等实体页生效 `[实测]`。

### 1.3 推理 API 接入参数

| 项目 | 值 | 证据 |
| --- | --- | --- |
| Base URL | `https://api-inference.modelscope.cn/v1` | `[实测]` `/v1/models` 返回 OpenAI 格式模型列表 |
| 是否 OpenAI 兼容 | **是** | `[实测]` 返回 `{"object":"list","data":[{"id":...,"object":"","owned_by":"system","created":...}]}` |
| 认证方式 | `Authorization: Bearer <Access Token>` | `[实测]` 无鉴权 POST `/v1/chat/completions` 返回 **401** |
| 模型 id 命名 | `组织/模型名`，**带斜杠**，与 HuggingFace 同构 | `[实测]` 见下表 |

**实测可用模型 id（`/v1/models` 全量 37 个，摘录有代表性的）**：

```
Qwen/Qwen3.5-397B-A17B        Qwen/Qwen3.5-122B-A10B
Qwen/Qwen3.5-35B-A3B          Qwen/Qwen3.5-27B
Qwen/Qwen3.8-27B              Qwen/Qwen3.8-Flash-Next
Qwen/Qwen-Image-Edit
deepseek-ai/DeepSeek-V4-Pro   deepseek-ai/DeepSeek-V4.1-Flash
ZhipuAI/GLM-5.2               ZhipuAI/GLM-4.7-Flash
MiniMax/MiniMax-M3            MiniMax/MiniMax-M1-80k
stepfun-ai/Step-5-Preview     PaddlePaddle/ERNIE-4.5-300B-A47B-PT
mistralai/Mistral-Large-Instruct-2407
```

> 注意：模型 id **必须带 `组织/` 前缀**（如 `Qwen/Qwen3.8-Flash-Next`），这与多数厂商的扁平命名不同，网关做模型映射时不要剥掉前缀。

### 1.4 免费额度规则

| 规则 | 内容 | 证据强度 |
| --- | --- | --- |
| 基础免费额度 | **每模型每日 2000 次** API-Inference 调用，覆盖平台上大部分开源模型 | `[文档]` 来自第三方汇总与官方 limits 页标题；官方页正文本次**未能抓到**（SPA 渲染），见下方「未核实项」 |
| 热门模型单独限额 | 部分热门模型另有更低上限（例如 Qwen3-Coder 系列曾限 **500 次/日**） | `[文档]` 第三方汇总，官方页未直接确认 |
| 绑定阿里云账号 | **必须绑定已完成实名认证的阿里云账号**才能调用 | `[文档]` 多方一致，可信度高 |
| 计费单位 | 平台同时存在「**魔粒**」（MoLi）积分体系，部分调用按 0.5 / 1 / 2 魔粒扣减 | `[推测]` 站点 bundle 内确认存在「魔粒」字样（1 处命中）`[实测]`；具体扣减规则未核实 |
| 额度重置 | 每日重置，平台可随时调整 | `[文档]` |

> **未核实项（需人工打开浏览器确认）**：`https://www.modelscope.cn/docs/model-service/API-Inference/limits` 的正文。该页是 SPA 客户端渲染，`pwsh` 抓到的始终是空壳 HTML（正文长度为 9 字符）。上面的「2000 次/日」等数字来自搜索摘要与第三方汇总，**请 Lead 在写文档时标注为「参考值，以官方页面为准」**。

### 1.5 建议的快捷连接条目（ModelScope）

```jsonc
{
  "name": "魔搭 ModelScope",
  "keyPage": "https://www.modelscope.cn/my/settings/token",   // 新地址
  "console": "https://www.modelscope.cn/my/settings/token",
  "docs": "https://www.modelscope.cn/docs/model-service/API-Inference/intro",
  "baseUrl": "https://api-inference.modelscope.cn/v1",
  "openaiCompatible": true,
  "auth": "bearer"
}
```

---

## 2. Agnes AI（用户口中的「agens」）

### 2.1 身份确认：为什么是 Agnes AI

| 证据 | 内容 | 强度 |
| --- | --- | --- |
| 仓库代码交叉印证 | `src/protocol/openai.rs:437`「部分上游（如 **agnes**）要求 user/system/tool 消息必须带 content 字段」；`src/protocol/content.rs:128`「**agnes 的 vLLM 部署**无法处理单通道图片」；`handler.rs:1077` 同一条 | **强** `[实测]`（grep 仓库） |
| 模型命名 | 仓库 mockup / e2e 中出现 `agnes-3.0-flash`、`agnes-5`；Agnes AI 官方定价页**确实**列出 `agnes-3.0-flash`、`agnes-2.5-flash`、`agnes-2.5-pro` 等 | **强** `[文档]` |
| 双站结构 | 用户说「海内外」，Agnes AI 恰好有 `agnes-ai.com`（国际站）+ `agnes-ai.cn`（中国站），且两套文档/定价分开 | **强** `[实测]` |
| 排除项 | `agnesai.com` / `www.agnesai.com` 是**停放页**（返回 114 字节跳转脚本 `window.location.href="/lander"`），**不是**该产品 | `[实测]` |

> **结论**：用户说的「agens」= **Agnes AI**（a-g-n-e-s）。`agnesai.com` 是无关的停放域名，不要写进文档。

### 2.2 双站对照表（关键：base_url 不同）

| 项目 | 海外站 / 国际站 | 国内站 / 中国站 |
| --- | --- | --- |
| 官网 | `https://agnes-ai.com` | `https://agnes-ai.cn` |
| 开发者控制台 | `https://platform.agnes-ai.com` | `https://platform.agnes-ai.cn` |
| **API Key 申请页** | **`https://platform.agnes-ai.com/settings/apiKeys`** | **`https://platform.agnes-ai.cn/settings/apiKeys`** |
| **OpenAI 兼容 base_url** | **`https://apihub.agnes-ai.com/v1`** | **`https://api.agnes-ai.cn/v1`** |
| 文档站 | `https://wiki.agnes-ai.com` | `https://wiki.agnes-ai.cn` |
| 定价页 | `.../pricing.md`（**美元**） | `.../pricing.md`（**人民币**） |
| 订阅/积分 | Token Plan（按请求次数配额） | **Agent Plan**（按 Credits 积分） |

> ⚠️ **这是本次调研最重要的发现**：两站 base_url **不同**，不是同一网关的两个域名。国内站文档 `overview.md` 明写 `https://api.agnes-ai.cn/v1`，国际站 `overview.md` 明写 `https://apihub.agnes-ai.com/v1`。**网关需要建两个独立平台**（各自的 base_url + 各自的 Key），不能只配一个。
>
> 另注：`apihub.agnes-ai.cn` 也能解析（返回 200 外壳 / 401），但**官方文档从未提及**，属实现细节，`[推测]` 不建议写进配置。

### 2.3 URL 可达性实测（Agnes）

| URL | 用途 | 可达性（实测） | 备注 |
| --- | --- | --- | --- |
| `https://agnes-ai.com` | 海外站官网 | **200** | `[实测]` title=`Agnes AI \| Free Omni-Modal AI API, World-Class AI Models & AI Applications` |
| `https://agnes-ai.cn` | 国内站官网 | **200** | `[实测]` 同上 title |
| `https://platform.agnes-ai.com` | 海外站控制台 | **200** | `[实测]` title=`Agnes`，Next.js SPA |
| `https://platform.agnes-ai.cn` | 国内站控制台 | **200** | `[实测]` title=`Agnes` |
| **`https://platform.agnes-ai.com/settings/apiKeys`** | **海外站 API Key 申请页** | **200** | `[实测]` **精确路径**，见下方取证说明 |
| **`https://platform.agnes-ai.cn/settings/apiKeys`** | **国内站 API Key 申请页** | **200** | `[实测]` |
| `https://platform.agnes-ai.com/settings/usage` | 用量页 | **200** | `[实测]` |
| `https://platform.agnes-ai.com/billing/balance` | 余额/账单 | **200** | `[实测]` |
| `https://platform.agnes-ai.com/subscribe/subscription` | 订阅页（文档中多次引用） | **200** | `[实测]` 文档写的是带 `?from=website` 的版本，同样 200 |
| `https://platform.agnes-ai.com/subscribe/codingPlan` | Coding Plan 订阅 | **200** | `[实测]` |
| `https://platform.agnes-ai.com/notifications` | 通知中心 | **200** | `[实测]` |
| `https://wiki.agnes-ai.com` | 文档站（海外，Mintlify） | **200** | `[实测]` |
| `https://wiki.agnes-ai.cn` | 文档站（国内） | **200** | `[实测]` 与 .com 内容对应 |
| `https://wiki.agnes-ai.com/llms.txt` | **文档全量索引（纯文本，最好用）** | **200** | `[实测]` 4704 字节，列出全部页面 |
| `https://wiki.agnes-ai.com/_llms/zh-hans.txt` | 中文文档索引 | **200** | `[实测]` |
| `https://apihub.agnes-ai.com/v1/models` | 海外 base_url 探活 | **401** | `[实测]` 401 = 端点存在、需鉴权 |
| `https://api.agnes-ai.cn/v1/models` | 国内 base_url 探活 | **401** | `[实测]` 同上 |
| ~~`https://agnesai.com`~~ | — | 200 但**是停放页** | `[实测]` 返回 114 字节 `window.location.href="/lander"`，**非目标产品** |

**`/settings/apiKeys` 的取证过程**（因为 SPA 的 404 是真实 404，可以据此反推有效路径）：
- `[实测]` 该平台是 Next.js，**未知路径返回真实 HTTP 404**（如 `/apikey`、`/api-keys`、`/keys`、`/console`、`/dashboard`、`/settings` 全部 404）。
- `[实测]` 从控制台 JS chunk 的侧边栏配置里直接读到路由表：
  `{key:"/settings/apiKeys", label:"API Keys"}`、`{key:"/settings/usage"}`、`{key:"/settings/profile"}`、`{key:"/billing/balance"}`、`{key:"/billing/billing"}`、`{key:"/subscribe/codingPlan"}`、`{key:"/notifications"}`。
- `[实测]` 逐条访问，全部 200。**`/settings/apiKeys` 是文档里没写、但代码里唯一的 Key 管理入口。**

### 2.4 接入参数

| 项目 | 海外站 | 国内站 | 证据 |
| --- | --- | --- | --- |
| base_url | `https://apihub.agnes-ai.com/v1` | `https://api.agnes-ai.cn/v1` | `[文档]` 各自 `overview.md` + `quickstart.md` 的 curl 示例 |
| 认证 | `Authorization: Bearer YOUR_API_KEY` | 同 | `[文档]` overview.md「6. 认证」 |
| OpenAI 兼容 | **是** | 是 | `[文档]` overview.md「4. API 兼容性：兼容 OpenAI 风格的接口」 |
| 端点 | `/chat/completions`、`/responses`、`/messages` | 同 | `[文档]` agnes-30-flash.md 的 API Endpoints 表 |
| 图像/视频 | `/images/generations`、`/videos`（OpenAI Videos 兼容） | 同 | `[文档]` faqs.md |
| 自定义 header | **无**，标准 Bearer | 同 | `[文档]` 未提及任何自定义 header |

### 2.5 模型清单与规格（海外站定价页 + 模型页）

| 模型 id | 类型 | 上下文 | 最大输出 | 定价（美元/百万 Token，现价） |
| --- | --- | --- | --- | --- |
| `agnes-3.0-flash` | 文本（Agent/编码） | **512K** | **65,536** | 输入 $0 / 输出 $0（**限时免费**） |
| `agnes-2.5-flash` | 文本 | — | — | 输入 $0 / 输出 $0（限时免费） |
| `agnes-2.5-pro` | 文本 | — | — | 输入 $0.45 / 输出 $0.90 |
| `agnes-2.5-pro-beta` | 文本 | — | — | 输入 $0.10 / 输出 $0.30 |
| `agnes-3.0-pro` | 文本 | — | — | 输入 $0.45 / 输出 $0.90（**即将上线**） |
| `agnes-image-2.0-flash` / `2.1-flash` / `2.5-flash` | 图像 | — | — | 当前 $0（1K/2K/3K/4K 档位） |
| `agnes-video-2.5` / `agnes-video-2.5-flash` | 视频 | — | — | 按秒/分辨率计费 |

`agnes-3.0-flash` 支持的请求字段 `[文档]`：`model`、`messages`（含 `image_url` 内容块）、`temperature`、`top_p`、`max_tokens`、`stream`、**`tools`**、**`tool_choice`**、`chat_template_kwargs`（Thinking 等扩展）。
→ **支持 tools / 工具调用**；thinking 通过 `chat_template_kwargs` 传，不是标准 `reasoning_effort`。这与仓库里 `src/protocol/content.rs` 对 agnes 的特殊处理相吻合。

### 2.6 额度 / 积分规则（两站机制不同！）

**国内站 `agnes-ai.cn` —— Agent Plan（Credits 积分制）** `[文档]`

| 套餐 | 月付 | 年付 | 套餐 Credits/月 | 5 小时窗口 | 文本 RPM |
| --- | --- | --- | --- | --- | --- |
| Lite | ¥19.90 | ¥199 | 20,000 | 2,000 | 60 |
| Plus | ¥99.90 | ¥999 | 100,000 | 10,000 | 100 |
| Pro | ¥199.90 | ¥1,999 | 240,000 | 24,000 | 200 |
| Max | ¥500.00 | ¥5,000 | 600,000 | 60,000 | 500 |

- **换算基准：1,000 Credits = ¥1**（仅折算基准，非售卖单价）
- 扣费公式：`本次扣除 Credits = 模型刊例价对应 Credits × 套餐折扣系数 + 不参与折扣的输入图片超额费用`
- 折扣系数：文本 Flash 1.0/0.9/0.8/0.7，文本 Pro 1.0/0.95/0.9/0.85，图片 1.0/0.98/0.95/0.9，视频 1.0/1.0/0.98/0.95（Lite/Plus/Pro/Max）
- 加购积分包：Mini 20,000/¥19.90、Standard 100,000/¥99.90、Premium 240,000/¥229、Ultimate 600,000/¥569；**永久有效、不受 5 小时窗口约束**
- **免费用户文本 RPM = 10**，企业认证用户 = 20（2026-06-22 起统一下调 50%）

**海外站 `agnes-ai.com` —— Token Plan（请求次数配额制）** `[文档]`

| Plan | `agnes-3.0-flash` | 图片 | 视频 |
| --- | --- | --- | --- |
| Starter | 每 5 小时 1,500 次；每周 15,000 次 | 4,000 张/天 | 500 秒/天 |
| Plus | 每 5 小时 7,500 次；每周 75,000 次 | 4,000 张/天 | 500 秒/天 |
| Pro | 每 5 小时 30,000 次；每周 300,000 次 | 4,000 张/天 | 500 秒/天 |

- 免费/默认用户文本 RPM = **10**，企业用户 20，Token Plan 1000
- 免费 Key 与 Token Plan Key 使用**不同的限制池**，多建 Key 不叠加额度

### 2.7 网关做 429 / 402 映射需要的错误码 `[文档]`

| 状态码 | 含义 | 网关建议 |
| --- | --- | --- |
| **402** | 余额/配额不足（余额、Token Plan 配额、订阅状态） | **Agnes 特有**——额度耗尽返回 **402 而非 429**，网关要单独映射为「额度耗尽」，不要当成永久失败 |
| **429** | 超出 RPM（免费用户 RPM 10/20） | 按限流重试 |
| 401 | Key 无效/过期/缺 Bearer 空格 | 永久失败，不重试 |
| 403 | 无该模型权限 / 地区受限 | 永久失败 |
| 404 | base_url 错误、**`/v1` 被重复添加**、模型名错 | 检查 base_url 拼接 |
| 413 | 请求体过大（base64 图片） | 建议改传公网 URL |

> 特别注意 404 的成因里官方明确写了「`/v1` 被第三方工具重复添加」——网关做 base_url 规范化时要小心。

---

## 3. 「快捷连接」通用预设清单（含实测状态码）

> 所有状态码均为 2026-10-27 用 `pwsh` 实际请求所得。Key 申请页普遍是 SPA，**未登录时会渲染登录页但返回 200**，这属于正常（外链跳转可用）。
> `base_url` 一列的 401 是**故意不带 Key 请求 `/v1/models` 的结果**——401 恰好证明「端点存在且要鉴权」，比 200 更能说明 base_url 正确。

### 3.1 主表

| # | 厂商 | Key 申请页 URL | 实测 | OpenAI 兼容 base_url | base_url 实测 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **魔搭 ModelScope** | `https://www.modelscope.cn/my/settings/token` | **200** | `https://api-inference.modelscope.cn/v1` | **200**（`/models` 免鉴权） | 历史地址 `/my/myaccesstoken` 已 302→ 新地址 |
| 2 | **Agnes AI（海外）** | `https://platform.agnes-ai.com/settings/apiKeys` | **200** | `https://apihub.agnes-ai.com/v1` | **401** | 美元计价，Token Plan |
| 3 | **Agnes AI（国内）** | `https://platform.agnes-ai.cn/settings/apiKeys` | **200** | `https://api.agnes-ai.cn/v1` | **401** | 人民币计价，Agent Plan 积分 |
| 4 | OpenAI | `https://platform.openai.com/api-keys` | **403** ⚠️ | `https://api.openai.com/v1` | **401** | 403 是 **Cloudflare 拦机房/无头 UA**，浏览器正常。**未实测到 200** |
| 5 | Anthropic | `https://console.anthropic.com/settings/keys` | **200** | `https://api.anthropic.com/v1`（原生） | **401** | 非 OpenAI 兼容；OpenAI 兼容需走 `https://api.anthropic.com/v1/` 的 messages 协议 |
| 6 | DeepSeek | `https://platform.deepseek.com/api_keys` | **200** | `https://api.deepseek.com/v1` | **401** | 也可用 `https://api.deepseek.com`（免 `/v1`） |
| 7 | 智谱 BigModel | `https://bigmodel.cn/usercenter/proj-mgmt/apikeys` | **200** | `https://open.bigmodel.cn/api/paas/v4` | **401** | 国内站；`open.bigmodel.cn/usercenter/apikeys` 也 200 |
| 8 | Moonshot / Kimi | `https://platform.moonshot.cn/console/api-keys` | **200** | `https://api.moonshot.cn/v1` | **401** | 海外版 `platform.moonshot.ai/console/api-keys` 也 **200**，base `https://api.moonshot.ai/v1` **401** |
| 9 | SiliconFlow 硅基流动 | `https://cloud.siliconflow.cn/account/ak` | **200** | `https://api.siliconflow.cn/v1` | **401** | 海外版 `cloud.siliconflow.com/account/ak` **200**，base `https://api.siliconflow.com/v1` **401** |
| 10 | OpenRouter | `https://openrouter.ai/keys` | **200** | `https://openrouter.ai/api/v1` | **200**（`/models` 免鉴权） | 模型列表公开可读 |
| 11 | 阿里百炼 DashScope | `https://bailian.console.aliyun.com/?apiKey=1` | **200** | `https://dashscope.aliyuncs.com/compatible-mode/v1` | **401** | 兼容模式路径必须带 `compatible-mode` |
| 12 | 火山方舟 Ark | `https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey` | **200** | `https://ark.cn-beijing.volces.com/api/v3` | **401** | 注意是 `/api/v3` 不是 `/v1` |
| 13 | Groq | `https://console.groq.com/keys` | **200** | `https://api.groq.com/openai/v1` | **401** | 路径含 `/openai` |
| 14 | Together AI | `https://api.together.xyz/settings/api-keys` | **200** | `https://api.together.xyz/v1` | **401** | 新域名 `api.together.ai/settings/api-keys` 也 **200** |
| 15 | 腾讯云 LKEAP | `https://console.cloud.tencent.com/lkeap/api-key` | **200** | `https://api.lkeap.cloud.tencent.com/v1` | **401** | 需腾讯云账号 |

### 3.2 逐条证据说明

- **#1 ModelScope**：`[实测]` Key 页 200；`/v1/models` **200 且返回真实模型列表**（37 条）——这是全表唯一一个免鉴权就能验证 base_url 正确的。
- **#2/#3 Agnes**：`[实测]` 两站 `/settings/apiKeys` 均 200；两站 `/v1/models` 均 401。**注意两家 base_url 不同**，见 §2.2。
- **#4 OpenAI**：`[实测]` `platform.openai.com/api-keys` 与 `/settings/organization/api-keys` 均返回 **403**（`curl` 与 `Invoke-WebRequest` 结果一致）。这是 Cloudflare 针对非浏览器 UA 的拦截，**URL 本身是有效的**，但不能声称「已核实可达」。文档里请标「未实测（403 为反爬拦截）」。
- **#5 Anthropic**：`[实测]` Key 页 200（title `API keys | Claude Platform`）。注意 Anthropic 是**原生 Messages 协议**，不是 OpenAI 兼容；网关若要用它需要协议转换（本项目 `src/protocol/anthropic.rs` 已实现）。
- **#6 DeepSeek**：`[实测]` 200（title `DeepSeek`）。页面很短（2707 字节），是 SPA 外壳。
- **#7 智谱**：`[实测]` 200（title 为 GBK 乱码 `æ™ºè°±ä¸¨BigModel å¹³å°`，即「智谱·BigModel 平台」，说明页面是 UTF-8 但被按 GBK 解码，不影响可达性）。
- **#8 Moonshot**：`[实测]` 200（title `Kimi API 开放平台`）。国内外两套域名均已验证。
- **#9 SiliconFlow**：`[实测]` 200（title `硅基流动统一登录`）。国内外两套域名均已验证。
- **#10 OpenRouter**：`[实测]` Key 页 200（`Sign In | OpenRouter`）；`/api/v1/models` **200 且返回 784 KB 模型列表**，免鉴权。
- **#11 百炼**：`[实测]` 200（title `大模型服务平台百炼控制台`）。带 `?apiKey=1` 参数可直接定位到 Key 管理。
- **#12 火山方舟**：`[实测]` 200（页面 439 KB，title 为空，是控制台 SPA）。`region:ark+cn-beijing` 这段路径是必需的。
- **#13 Groq**：`[实测]` 200（title `API Keys - GroqCloud`）——**全表标题最明确的一个**。
- **#14 Together**：`[实测]` 200（`Sign In · Together AI`）。旧域名 `api.together.xyz` 与新域名 `api.together.ai` 都可用。
- **#15 腾讯云**：`[实测]` 200（title `登录 - 腾讯云`，即未登录渲染登录页，正常）。

### 3.3 未覆盖 / 存疑项

| 项 | 情况 |
| --- | --- |
| OpenAI Key 页 | 只拿到 **403**（反爬），**未能实测 200**。建议 Lead 标注为「地址正确但本次未实测通过」 |
| 各家「控制台」二级 URL | 除 Agnes（已从 JS 路由表精确拿到 `/settings/apiKeys`）外，其余厂商只验证到 Key 页这一层，未深挖控制台内其他路径 |
| 阿里百炼 / 火山方舟的 base_url | 只验证到 401（端点存在），未验证真实调用，因无 Key |

---

## 4. 给网关实现的建议

### 4.1 数据结构

```ts
type QuickLink = {
  id: string;                 // "modelscope" | "agnes-intl" | "agnes-cn" | "openai" ...
  name: string;               // 展示名
  region?: "cn" | "intl";     // 国内站/海外站，用于分组
  keyPage: string;            // 快捷连接跳转目标
  console?: string;           // 控制台首页
  docs?: string;              // 文档
  baseUrl: string;            // OpenAI 兼容 base_url
  openaiCompatible: boolean;
  auth: "bearer" | "header";
  verified: boolean;          // 本次是否实测到 200
  note?: string;              // 例如 "额度耗尽返回 402"
};
```

### 4.2 三个容易踩的点

1. **Agnes 必须拆成两条记录**（`agnes-intl` / `agnes-cn`），因为 base_url 不同。如果 UI 上想合成一张卡片，也必须在选站之后再决定 base_url。
2. **ModelScope 的模型 id 带斜杠**（`Qwen/Qwen3.8-Flash-Next`），网关模型映射 UI 不要做「自动补全组织名」或「去斜杠」。
3. **Agnes 额度耗尽返回 402**，不是 429。网关的额度/限流分类逻辑要加这条，否则会把「额度用完」误判成「服务端错误」反复重试。

### 4.3 外链打开方式

本项目是 Tauri 桌面应用 + Web 端双形态，外链不能用 `window.open`（Tauri WebView 会吞掉或开在内嵌窗口）。建议复用已有的 `shell:allow-open` 能力（见 `src-tauri/capabilities/default.json` 里的 `shell:allow-open` 权限），走 `@tauri-apps/plugin-shell` 的 `open()`；Web 端回退到 `window.open(url, "_blank", "noopener")`。

---

## 5. 核实方法与局限（供 Lead 判断可信度）

**已实测**：本文件中出现的每一个 HTTP 状态码，都是本次会话用 `pwsh` 的 `Invoke-WebRequest` / `curl.exe` 真实发起的请求结果。

**局限**：
1. `web_search` / `web_fetch` 工具本次**完全不可用**（所有后端 403），检索改用 `pwsh` 调 Bing / DuckDuckGo HTML 端点。Bing 对部分中文长查询返回了**不相关结果**（例如搜「魔搭…免费额度」返回了汉字释义），这类查询的结论已降级为 `[文档]` 或标注未核实。
2. **SPA 的 200 不等于路径有效**。ModelScope（umi）和 Agnes 控制台（Next.js）行为相反：前者对任意 `/my/*` 都返 200，后者对未知路径返真实 404。因此：
   - ModelScope 的结论**依赖 `umi.js` 路由表**（已下载 10.4 MB bundle 并 grep 到 `redirect:"/my/settings/token"`）。
   - Agnes 的结论**依赖 Next.js chunk 侧边栏路由表**（已下载 34 个 chunk 并 grep 到 `key:"/settings/apiKeys"`）。
3. **ModelScope API-Inference 免费额度的官方正文未能抓到**（该页客户端渲染，抓到的 HTML 正文只有 9 字符）。「2000 次/日」「Qwen3-Coder 500 次/日」等数字来自搜索摘要，**建议在正式文档中标注为参考值**。
4. OpenAI 的 Key 页只拿到 403，**未能实测 200**，已在表中明确标注。

---

## 6. 参考链接汇总

- ModelScope 文档中心：https://www.modelscope.cn/docs
- ModelScope API-Inference 介绍：https://www.modelscope.cn/docs/model-service/API-Inference/intro
- ModelScope API-Inference 使用限制：https://www.modelscope.cn/docs/model-service/API-Inference/limits
- ModelScope 站点 llms.txt（品牌名/域名规范）：https://www.modelscope.cn/llms.txt
- Agnes 文档索引（海外）：https://wiki.agnes-ai.com/llms.txt
- Agnes 文档索引（国内）：https://wiki.agnes-ai.cn/llms.txt
- Agnes 概述（海外，含 base_url）：https://wiki.agnes-ai.com/zh-Hans/docs/overview.md
- Agnes 概述（国内，含 base_url）：https://wiki.agnes-ai.cn/zh-Hans/docs/overview.md
- Agnes 快速开始：https://wiki.agnes-ai.com/zh-Hans/docs/quickstart.md
- Agnes 模型定价（海外/美元）：https://wiki.agnes-ai.com/zh-Hans/docs/pricing.md
- Agnes 模型定价（国内/人民币）：https://wiki.agnes-ai.cn/zh-Hans/docs/pricing.md
- Agnes 订阅计划（国内 Agent Plan 积分）：https://wiki.agnes-ai.cn/zh-Hans/docs/tokenplan.md
- Agnes Token Plan（海外）：https://wiki.agnes-ai.com/zh-Hans/docs/tokenplan.md
- Agnes 常见错误码：https://wiki.agnes-ai.com/zh-Hans/docs/code.md
- Agnes 3.0 Flash 模型页：https://wiki.agnes-ai.com/zh-Hans/docs/agnes-30-flash.md
- Agnes 常见问题：https://wiki.agnes-ai.com/zh-Hans/docs/faqs.md
