<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>一个网关，统一接入所有 AI 平台</strong><br/>
  多 Key 负载均衡 · 失败重试与换后端 · OpenAI &amp; Anthropic 双协议 · 客户端零改造
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-6.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
  <a href="https://github.com/keiskeies/ai-gateway/stargazers"><img src="https://img.shields.io/github/stars/keiskeies/ai-gateway?style=social" alt="Stars" /></a>
</p>

<p align="center">
  <strong>中文</strong> ·
  <a href="README_EN.md">English</a> ·
  <a href="README_DE.md">Deutsch</a> ·
  <a href="README_FR.md">Français</a> ·
  <a href="README_ES.md">Español</a> ·
  <a href="README_PT.md">Português</a>
</p>

<p align="center">
  <a href="#-快速开始"><strong>快速开始 →</strong></a>
</p>

---

> 🌟 如果这个项目对你有帮助，欢迎点个 Star。它能让更多人找到这个项目。

---

## 😩 它解决什么问题

- 同一平台有多把 Key，想合起来用，却只能自己写轮询和重试
- 429 / 5xx / 上游超时想自动重试，不想在每个客户端里重复实现
- OpenAI 和 Anthropic 两套 SDK、两套 Base URL、两套消息格式
- 免费额度和付费模型混用，成本、配额、失败原因分散在各处
- 想给团队发 Key，又不想把上游平台密钥交出去

AI Gateway 把这些放进一个本地进程：**一个入口、一套 Key、一份统计**。

---

## 📸 界面

以下截图为当前桌面端的实际运行界面（深色主题）。

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="监控大屏" />
</p>
<p align="center"><em>监控大屏 — KPI 条 + 每模型趋势（请求 / 失败 / 输入 / 输出 / 延时 / 首字 / 速度 / 缓存八项指标，时·日·月·年四档粒度）、活跃度热力图（7d / 30d / 12m / All）、平台占比、模型性能矩阵与请求日志。支持全屏和暂停刷新</em></p>

<details>
<summary>📱 展开其余页面截图</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="流量" />
</p>
<p align="center"><em>流量 — 顶部「需要关注」汇总限流 / 失败 / 无流量三类异常；请求表可按模型、平台、密钥、状态、错误类型、延时区间筛选并分页回溯；右侧单笔详情含尝试链（每次后端尝试的状态与耗时）、TTFT / 生成耗时条、缓存读与新增输入、预估成本，并可复制 ID、重放请求</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="虚拟模型" />
</p>
<p align="center"><em>虚拟模型 — 列表直接给出健康状态、后端数、负载策略、请求量、成功率、平均延时与成本</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="虚拟模型路由" />
</p>
<p align="center"><em>模型详情 · 路由 — 左侧改策略、重试次数、退避、默认 max_tokens 与权重分布；中间「路由脊柱」把后端按权重与优先级画成连线图；右侧查看单个后端的供应商、能力标记与思考参数处理方式</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="平台" />
</p>
<p align="center"><em>平台 — 连通性检测结果、已同步模型数、被多少虚拟模型引用、请求量与成功率；顶栏汇总异常的平台数量</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="平台详情" />
</p>
<p align="center"><em>平台详情 — 凭据与转发头（已保存的 Key 只返回掩码，不会回传明文）、测试连接时拉取远程模型、从平台勾选导入模型、定价 CSV 批量导入导出，以及「哪些虚拟模型在用这个平台」</em></p>

<p align="center">
  <img src="doc/console-keys.png" width="92%" alt="API 密钥" />
</p>
<p align="center"><em>API 密钥 — 每把 Key 的请求数、Token 数、平均延时、成本与最后使用时间；可绑定到单个虚拟模型</em></p>

<p align="center">
  <img src="doc/console-key-quota.png" width="92%" alt="密钥配额" />
</p>
<p align="center"><em>密钥配额 — 到期时间、请求数上限、Token 上限、成本预算、配额窗口（终身 / 日 / 周 / 月）。触达任一上限返回 429；成本预算只对已定价模型生效</em></p>

<p align="center">
  <img src="doc/console-playground.png" width="92%" alt="Playground" />
</p>
<p align="center"><em>Playground — 选虚拟模型和协议（OpenAI / Anthropic）发一笔真实请求，验证路由与上游；可就地生成一把临时密钥并填好，或复制等价 cURL</em></p>

<p align="center">
  <img src="doc/console-clients.png" width="92%" alt="客户端接入" />
</p>
<p align="center"><em>客户端接入 — Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes 六家本地配置的卡片式接入；展开后自动显示差异，应用前备份，接入后可一键撤销</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="设置" />
</p>
<p align="center"><em>设置 — 服务端口与监听地址、请求策略默认值、账号安全、数据统计（行数 / 尝试轨迹 / 告警 / 悬空行 / 正文留存）、配置导出导入、整库备份与恢复</em></p>

</details>

---

## ⚡ 核心能力

### 🎯 多 Key：把同一供应商的多把 Key 合成一个入口

网关里一个「平台」保存一条上游凭据，一个「虚拟模型」下有多个后端。要把多把 Key 合并使用：

```
同一供应商建多个平台（每个填一把 Key）→ 一起加到同一个虚拟模型 → 选轮询或最少连接
```

```
你的应用
   │
   └──→ 虚拟模型 ──┬── 平台 A（Key 1，3 RPM）
                    ├── 平台 B（Key 2，3 RPM）
                    └── 平台 C（Key 3，3 RPM）
```

吞吐上限是各 Key 限额之和；网关不会绕过上游自己的限流。

### 🔀 负载均衡策略

| 策略 | 行为 | 适合 |
|------|------|------|
| **轮询** | 在一个虚拟模型的后端之间依次分配 | 多 Key / 多账号叠加额度 |
| **加权随机** | 按 `weight` 概率选择 | 后端性能不一致 |
| **最少连接** | 选当前等待上游响应头的请求最少的后端 | 缓解短请求的瞬时并发 |
| **优先级** | 只选 `priority` 数值最小的 Active 后端 | 固定主用后端 |

> 「延迟优先」在界面上可选，但目前与轮询走同一实现，延迟数据尚未参与选择；「优先级」也不会在主用后端失败时自动降级。详见 [已知边界](#-已知边界)。

### 🌐 OpenAI &amp; Anthropic 双协议

```
OpenAI SDK / OpenAI 兼容工具 ──→ POST /v1/chat/completions ──┐
                                                              ├──→ 路由 → 上游
Anthropic SDK ─────────────────→ POST /v1/messages ──────────┘
```

| 路径 | 协议 | 说明 |
|---|---|---|
| `/v1/chat/completions` | OpenAI | 主入口 |
| `/v1/completions` | OpenAI | 旧路径，按 chat 格式解析 |
| `/v1/messages` | Anthropic | Anthropic 原生格式 |
| `/v1/models` | OpenAI | 公开；返回虚拟模型与能力字段 |

同协议非流式响应会原样透传（只把 `model` 改回虚拟模型名），跨协议响应按统一格式重建，流式响应逐块转换。

### 🧠 思考模式：客户端说了算

客户端传什么思考参数，网关就翻译成上游认识的格式；客户端不传，网关不主动开启。

| 客户端传入 | 上游是 Claude | 上游是 OpenAI 兼容推理模型 |
|---|---|---|
| `reasoning_effort: "high"`（OpenAI） | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: "high"` |
| `thinking: {budget_tokens: 10000}`（Anthropic） | 原样保留；`max_tokens` 不足时压缩 budget | 映射为对应 effort |

- 每个后端可配置思考参数风格：`Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / 关闭
- `Auto` 按上游类型解析：Anthropic 用 `thinking`，其他平台用 `reasoning_effort`；`param_overrides` 最后合并并优先
- Anthropic 的 thinking 签名会原样回放；跨协议工具循环使用网关内的签名缓存（30 分钟，按 `tool_use.id` 关联），未命中时关闭思考而不是伪造签名
- `/v1/models` 对推理模型返回 `capabilities` / `supported_parameters` / `reasoning`；是否展示「思考程度」开关仍取决于客户端自己的模型能力表

### 🛡️ 重试与换后端

- 路由级重试：`max_retries` 和 `backoff_ms` 可在界面调整；重试错误类型固定为 429 / 5xx / 408
- 默认重试 **429 / 5xx / 408（上游超时响应）**，按指数退避（500ms × 2ⁿ）
- 每次重试重新经过负载均衡器，轮询下通常会落到下一个后端
- 流式请求单独使用更宽松的 `stream_timeout_secs`（默认 1800 秒），长生成不会被普通请求超时截断

> 连接级失败（后端不可达、拨号失败、网络层超时）默认不在重试列表里，也没有健康检查驱动的自动摘除。需要「后端宕机自动切走」时，请把后端放进同一虚拟模型的轮询 / 最少连接，或手动停用故障后端。详见 [已知边界](#-已知边界)。

### 📊 监控大屏 · 全链路性能统计

每个模型、每个时段（时 / 日 / 月 / 年）的完整性能画像：

- **8 项指标趋势**：请求 / 失败 / 输入 / 输出 / 延时 / 首字时长（TTFT）/ 输出速度（tok/s）/ 缓存命中
- **模型性能矩阵**：一屏看清所有模型的成功率、耗时、速度、缓存命中率，跟随选中指标排序
- **请求明细**：逐条记录输入 / 新增输入 / 输出 / 缓存读 / 耗时 / 首字 / 速度，支持模型与状态筛选、分页回溯
- **缓存命中率**：`命中 /（命中 + 新增输入）`，反映前缀缓存的真实收益
- **成本与覆盖率**：按定价表计算每笔请求成本，并显示已定价请求占比；没有价格的模型明确显示「未定价」

---

### 🧾 请求详情与保留期

- **一次请求一行明细**：入站路径、每次尝试的转发地址与上游模型、请求头、请求/响应正文、Token、TTFT、缓存、成本与积分；重试/换后端逐次可见
- **敏感信息不落库**：请求头只收白名单，`Authorization` / `x-api-key` / `Cookie` 永远排除；转发 URL 里的 `key` / `token` / `sig` 等 query 参数自动打码
- **正文采集默认关闭**：请求正文与响应正文各有独立开关，单条上限 64KB；流式响应保存上游原始 SSE，跨协议请求不会伪装成转换后的结果
- **保留期**：关闭或 30/90/180/365 天；启动时与每天自动清理，最短 30 天——更短会把 Key 的月度配额用量一起删掉

### 🎟️ 上游积分额度

- **平台总积分 + 模型单次积分价**：平台可设自然日/周/月预算（例如魔塔 250 分/天），模型可设每次请求消耗（1 分、2 分）
- **积分感知选路**：余额不足的平台自动跳过；全部耗尽时返回 429 并给出下次重置时间，不会静默发到已超额账号
- **独立台账**：扣费、释放、校准、重置都有流水；清理请求历史不会把已扣额度“退回来”
- **可见与可校准**：请求明细显示本次积分，平台列表/详情显示剩余与未定价模型数；低额度/耗尽进通知中心，支持手动校准

### 📚 共享模型目录与分时计费

- **同一个 URL 只配一次**：模型目录按「平台类型 + 归一化 Base URL」的哈希共享，同一地址再加一把 Key 会自动带上已有的模型列表、价格与积分价，不必重抓重填；目录改一处，同地址的所有账号一起生效
- **账号仍然拥有自己的行**：哈希只决定共享范围，虚拟模型与后端依旧挂在具体账号上；只有删掉该地址最后一个账号才清理目录，删除前模型会先迁给同目录的兄弟账号
- **缓存分档计价**：输入价拆成缓存未命中 / 缓存命中 / 缓存写入三档，Anthropic 的 `cache_read` 与 `cache_creation` 各自算钱；未命中价继续作为旧数据的兜底
- **峰谷价**：模型可配高峰时段（`HH:MM-HH:MM`，支持跨零点）和高峰四档价，按请求开始时刻选档，DeepSeek 的「夜间 0.02 / 白天 0.04」直接可配
- **历史成本不被追溯**：定价改动只影响之后的请求，已记录的每笔成本、统计与导出保持不变

## 🧱 已知边界

下面几条是当前实现的边界，容易被默认能力误导，先写清楚：

1. **「延迟优先」等同轮询**：策略选项存在，但选择逻辑仍是轮询，延迟没有参与排序（`src/lb/mod.rs`）。
2. **「优先级」不做失败降级**：只使用 `priority` 数值最小的 Active 后端；它失败后重试仍会选到它，其余后端平时不参与分流。
3. **没有健康检查驱动的自动摘除**：平台连通性检测用于展示和告警，不改变路由；路由的 `fallback` 字段尚未接入请求路径。
4. **连接失败默认不重试**：默认只重试 429 / 5xx / 408；网络层超时和拨号失败同属 `ConnectionError`，界面也不会把它写进重试列表。
5. **未创建 Key 也要带 Key**：一个 Key 都没建时，网关不校验 Key 的内容，但请求仍必须带 `Authorization` 或 `x-api-key` 请求头（内容不校验）；建了 Key 之后必须使用有效 Key。
6. **「最少连接」只统计到响应头**：连接计数在收到上游响应头时就释放，不覆盖流式正文的持续时间，因此它不是完整的流式并发数。
7. **请求/响应正文默认不开**：两个独立开关，单条上限 64KB；响应详情存的是上游原始响应，流式只留前 64KB，跨协议请求不是客户端最终看到的转换结果。
8. **自动清理最短 30 天**：关闭时只能手动清理；删除只释放 SQLite 页，不缩文件，需要另行 `VACUUM`。
9. **积分是本地台账**：按成功请求预扣/确认、失败释放，允许手动校准；窗口按服务器本地时区自然日/周/月，和上游账单可能有偏差。
10. **客户端接入只认本机控制台**：只有对端地址是回环、`Host` 也是回环、且请求不带 `Forwarded` / `X-Forwarded-*` / `X-Real-IP` 时 `/api/clients/*` 才放行，否则返回 403 且这一页不进菜单；它读写的是网关所在机器上的 `~/.claude`、`~/.codex`，远程浏览器无法代填。经本机 nginx 转发时仍要用 `127.0.0.1` 访问并去掉来源 IP 头（换成自定义域名同样会被判成远程）。
11. **共享目录按 URL 定价**：同一 Base URL 的多个账号共用一套模型、价格与积分价，没法给其中某一把 Key 单独定不同的价；确实要分开，就用不同的 URL 或不同的平台类型。
12. **峰谷档按服务器本地时区判定**：命中哪一档取决于请求开始时刻服务器所在时区的时间，与上游账单时区可能差几个小时；跨零点的窗口已支持，跨时区部署需要自己核对。

---

## 🆚 和其他方案的区别

| | AI Gateway | Nginx 反代 | One API | LiteLLM |
|---|---|---|---|---|
| **形态** | 桌面 App + 单可执行文件服务 | 通用反向代理 | 服务端 | Python 服务 |
| **可视化管理** | ✅ 内置控制台 | ❌ | ✅ Web 控制台 | ⚠️ 需启用 Admin UI |
| **负载均衡** | ✅ 4 种有效策略 + 延迟选项 | ✅ upstream 轮询 / 哈希 | ⚠️ 基础 | ✅ |
| **OpenAI 协议** | ✅ | ✅ 透传 | ✅ | ✅ |
| **Anthropic 协议** | ✅ 原生 | ❌ 只透传不转换 | ✅ 转换 | ✅ 转换 |
| **失败重试** | ✅ 429 / 5xx / 408 | ⚠️ 需配置 `proxy_next_upstream` | ✅ | ✅ 重试 + fallback |
| **远程模型列表** | ✅ 测试连接时拉取 | ❌ | ❌ | ⚠️ 以配置为准 |
| **桌面应用** | ✅ macOS / Windows / Linux | ❌ | ❌ | ❌ |
| **实现语言** | Rust | C | Go | Python |

> 对比按各项目公开文档和默认形态整理，具体以对应项目为准。

---

## ✨ 更多特性

- 🏪 **15 个平台预设**：OpenAI · Anthropic · DeepSeek · NVIDIA · Moonshot · ZhipuAI · Qwen · Doubao · SiliconFlow · Groq · Google Gemini · Together AI · OpenRouter · Ollama · Azure OpenAI。一键填入 Base URL 与模型清单，Key 仍需自备；也可手工填任意 OpenAI / Anthropic 兼容端点
- 🔍 **远程模型同步**：测试连接会拉取上游模型列表，同步时只补库里没有的模型，已有行的价格、能力和显示名不被覆盖
- 🧩 **共享模型目录**：模型与价格按「平台类型 + Base URL」哈希归并，重复接入同一地址不再重配；不同地址或不同平台类型仍是各自独立的目录
- 🏷️ **能力标记**：推理 / 视觉 / 嵌入 / 重排 / 工具，可按模型维护、批量修改；虚拟模型的能力徽标是各后端能力的交集
- 🔗 **reasoning_content 兼容**：支持 NVIDIA NIM 等平台的思维链输出；OpenAI / Anthropic 思考参数双向翻译
- 📡 **自定义转发头**：在平台级追加任意请求头，同名时覆盖 `Authorization` / `x-api-key`
- 🎛️ **Playground**：选模型、选协议、发真实请求，验证路由与上游；可就地生成临时 Key，或复制等价 cURL
- 🔌 **客户端接入**：Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes 六家本地配置一键接管与回滚；只改 AI Gateway 拥有的字段，应用前自动备份，支持差异预览与直接编辑配置文件；这一页只在**本机控制台**显示与放行，远程或经反向代理访问返回 403
- 🌙 **浅色 / 深色 / 跟随系统**三种主题；界面提供中文和英文
- 🧭 **八个页面**：流量 · 监控大屏 · 虚拟模型 · 平台 · API 密钥 · Playground · 客户端接入 · 设置。流量页的筛选、分页和时段写进地址，刷新与前进 / 后退不丢状态
- 🧾 **请求轨迹与正文重放**：一次请求的每次后端尝试逐条可查；正文留存默认关闭，开启后可原样重放某一笔请求
- 🔔 **通知中心**：Key 即将过期、平台连续失败、配额超阈值、备份过期都会落库，顶栏铃铛直接进入
- 💰 **成本估算与预算**：按定价表（含缓存三档与峰谷价）计算每次请求成本并给出覆盖率；Key 配额支持滚动窗口与超额拦截
- 🔑 **密钥级配额**：每把 Key 可设到期时间、请求数上限、Token 上限、成本预算与配额窗口（终身 / 日 / 周 / 月），并可绑定到单个虚拟模型
- 🗄️ **整库备份与恢复**：一键快照、下载、删除；原地还原前自动再留一份安全快照
- 📤 **配置与 CSV**：配置导出默认不含上游密钥（需要时打开对应开关），导入前可预览；流量按当前筛选导出 CSV，平台定价 CSV 批量进出
- ⚡ **实时刷新**：新请求和告警通过 SSE 推送，连接断开后自动退回 30 秒轮询
- 🖥️ **桌面端关窗不退出**：关闭窗口只销毁 WebView，网关继续服务；从托盘恢复窗口时会回到关闭前的页面

---

## 🔐 登录鉴权

管理后台（流量与统计、虚拟模型、平台、API 密钥、Playground、客户端接入、设置）需要登录：

- **首次启动**：打开管理页面会进入「初始化管理员账号」。初始化只接受来自本机（127.0.0.1）的请求，避免局域网内被他人抢先注册
- **会话**：登录状态保存在 HttpOnly、SameSite=Strict 的 Cookie 中，默认 7 天有效；右上角用户菜单可退出
- **修改密码**：设置 → 账号安全。修改后其他设备上的会话立即失效，当前设备会拿到一个新会话
- **忘记密码**：停止应用，删除数据库中 `users` 与 `sessions` 两张表的记录后重启，会重新进入初始化流程

| 路径 | 鉴权方式 |
|---|---|
| `/v1/chat/completions`、`/v1/completions`、`/v1/messages` | 网关 API Key（`Authorization: Bearer ...` 或 `x-api-key`） |
| `/v1/models` | 公开 |
| `/health` | 公开 |
| `/api/auth/status`、`/api/auth/login`、`/api/auth/setup` | 公开（`setup` 实际仅限本机） |
| 其余 `/api/*`（平台 / 模型 / 虚拟模型 / API Key / 客户端接入 / 设置 / 统计 / 备份） | 管理员登录会话 |

> 尚未创建任何 API Key 时，网关不校验 Key 的内容，但请求仍必须携带 `Authorization` 或 `x-api-key` 请求头（内容不校验）；创建 Key 之后必须使用有效 Key。账号密码保存在 SQLite 中（Argon2id 哈希），`config.toml` 的 `admin_token` 是历史保留字段。

---

## 🚀 快速开始

### 方式一：下载桌面应用（推荐）

前往 [Releases](https://github.com/keiskeies/ai-gateway/releases) 下载对应平台的安装包。桌面端支持 macOS（arm64 / x64）、Windows x64、Linux x64。

### 方式二：从源码构建

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# 先构建前端，产物输出到 ../static/
cd frontend && npm install && npm run build
cd ..

# 独立服务器模式
cargo run
# 访问 http://localhost:1994

# 或 Tauri 桌面应用模式
cargo install tauri-cli
cargo tauri dev
```

> 服务器模式直接读取 `static/`，所以从源码运行前需要先构建前端。前端构建会清空 `static/`。

### 四步上手

```
0️⃣ 初始化管理员账号 → 登录管理后台
1️⃣ 添加平台 → 选预设 + 填 API Key → 保存
2️⃣ 创建虚拟模型 → 选策略 + 添加后端 → 启用
3️⃣ 改 base_url → 指向 http://localhost:1994/v1 → 完成
```

需要合并多把 Key：同一供应商重复第 1 步，每个平台填一把 Key，第 2 步把它们都加到同一个虚拟模型。

---

## 📖 调用示例

```python
# OpenAI SDK — 只改 base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← 改这一行
    api_key="your-gateway-key"            # ← 网关里创建的 API Key
)
response = client.chat.completions.create(
    model="your-virtual-model",           # ← 虚拟模型名
    messages=[{"role": "user", "content": "hello"}]
)

# Anthropic SDK — 同样只改 base_url
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",     # ← 改这一行（SDK 会自己拼接 /v1/messages）
    api_key="your-gateway-key"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 curl 示例</summary>

```bash
# OpenAI 兼容格式
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_GATEWAY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic 兼容格式
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_GATEWAY_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# 虚拟模型列表（公开）
curl http://localhost:1994/v1/models
```

</details>

---

## 🎯 典型场景

| 场景 | 怎么做 | 结果 |
|------|--------|------|
| **免费额度叠加** | 同一供应商多账号建成多个平台，挂到同一虚拟模型并选轮询 | 在各 Key 的限额内叠加吞吐 |
| **成本控制** | 把高频任务指向便宜模型的虚拟模型，需要时再切到贵模型 | 贵的模型只处理真正需要的请求 |
| **稳定性** | 同一虚拟模型挂多个后端，开启 429 / 5xx / 408 重试 | 临时限流和上游抖动由网关消化 |
| **多平台混用** | OpenAI、Claude、Qwen 各建虚拟模型 | 一套 SDK 和 Base URL 全搞定 |
| **团队共享** | 统一网关 + 每把 Key 的配额与模型绑定 | 上游密钥不外发，用量可追踪 |

---

## 🏗️ 架构

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │  兼容端点 │  │  兼容端点  │  │   控制台     │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │   协议转换 · 路由 · 重试   │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N 个后端    │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
```

---

## ⚙️ 配置

`config.toml` 首次运行自动生成；设置页的改动会写回该文件。已存在的文件不会被覆盖。

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "ai-gateway-default-key-change-me"  # 请修改
admin_token = ""                                  # 历史保留字段

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
```

> 端口和监听地址的修改需要重启应用；其余设置立即生效。`capture_request_bodies` 控制是否把请求正文存入数据库，默认关闭。

---

## 🛠️ 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust 2021 · Actix-Web 4 · Reqwest |
| 数据库 | SQLite（rusqlite，`bundled` 编译，无需系统安装）· r2d2 连接池 |
| 前端 | React 19 · TypeScript 6 · Mantine 7 · Recharts · Vite 8 |
| 桌面端 | Tauri 2 |

---

## 🗺️ Roadmap

- [x] 📊 监控大屏：每模型全时段趋势 / 请求明细 / 缓存命中率
- [x] 🔑 API Key 细粒度配额：模型绑定 · 到期时间 · 请求 / Token / 成本三类上限 · 滚动窗口
- [x] 🔔 平台连通性检测与通知中心（检测结果只用于展示和告警，暂不参与路由）
- [x] 🧾 请求明细、正文留存与保留期 + 🎟️ 上游积分额度与台账
- [x] 🧩 共享模型目录（同 URL 账号复用）+ 💸 缓存三档与峰谷分时计费
- [ ] ⏱️ 「延迟优先」接入真实延迟采样
- [ ] 🩺 健康检查驱动的自动摘除与优先级降级
- [ ] 🧭 路由 `fallback` 接入请求路径
- [ ] 🌐 多模态网关（翻译 / TTS / Embedding 统一端点）
- [ ] 🐳 Docker 一键部署
- [ ] ☁️ 云端配置同步

> 有想要的功能？欢迎 [提 Issue](https://github.com/keiskeies/ai-gateway/issues)。

---

## 🤝 参与贡献

1. Fork 本仓库
2. 创建特性分支（`git checkout -b feature/amazing-feature`）
3. 提交更改（`git commit -m 'Add some amazing feature'`）
4. 推送到分支（`git push origin feature/amazing-feature`）
5. 发起 Pull Request

---

## 📂 项目结构

```
ai-gateway/
├── src/                  # Rust 后端（库 + 独立二进制）
│   ├── api/              # REST API 与管理接口
│   ├── db/               # 数据库层（r2d2 连接池）
│   ├── proxy/            # 代理 handler、流式转换、签名缓存
│   ├── lb/               # 负载均衡选择器
│   ├── protocol/         # OpenAI / Anthropic 协议适配与思考参数
│   ├── clients/          # 本地客户端配置接管（Claude Code / Codex / …）
│   └── models/           # 数据模型
├── frontend/             # React 前端（构建到 ../static/）
├── src-tauri/            # Tauri 桌面封装
├── e2e/                  # Playwright + pytest 端到端测试
├── doc/                  # 截图
├── config.toml           # 运行配置
└── data/                 # SQLite 数据库与备份
```

---

## ⭐ Star History

<p align="center">
  <a href="https://star-history.com/#keiskeies/ai-gateway&Date">
    <img src="https://api.star-history.com/svg?repos=keiskeies/ai-gateway&type=Date" alt="Star History Chart" width="600" />
  </a>
</p>

---

## 📜 License

[MIT License](LICENSE)

---

<p align="center">
  觉得有用？给个 <a href="https://github.com/keiskeies/ai-gateway">⭐ Star</a> 支持一下。
</p>
