<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>一个网关，聚合所有 AI 平台</strong><br/>
  多 Key 负载均衡 · 自动故障切换 · OpenAI & Anthropic 双协议 · 零代码改造
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
  <a href="https://github.com/keiskeies/ai-gateway/stargazers"><img src="https://img.shields.io/github/stars/keiskeies/ai-gateway?style=social" alt="Stars" /></a>
</p>

<p align="center">
  <a href="README.md">中文</a> | <a href="README_EN.md">English</a>
</p>

<p align="center">
  <a href="#-快速开始"><strong>30 秒上手 →</strong></a>
</p>

---

> 🌟 **如果这个项目对你有帮助，请给个 Star 支持一下！你的 Star 是我持续迭代的动力。**

---

## 😩 你是不是也遇到过这些问题？

- 🔴 API 调着调着就 **429 限流**了，请求直接报错
- 🔴 免费平台额度不够用，**3 RPM 根本不够塞牙缝**
- 🔴 DeepSeek 便宜但不稳定，OpenAI 稳定但太贵，**没法灵活切换**
- 🔴 多个 AI 平台 API 格式不同，**代码里一堆 if-else 切换**
- 🔴 想用 Claude 又想用 GPT，**SDK 和 Base URL 管不过来**

**AI Gateway 就是为了解决这些痛点而生的。**

---

## 📸 效果展示

以下截图均为 **v1.3.2** 实际运行界面（深色主题）。

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="监控大屏" />
</p>
<p align="center"><em>监控大屏 — KPI 条 + 每模型趋势（请求/失败/输入/输出/延时/首字/速度/缓存 八个指标，时·日·月·年四档粒度）、活跃度热力图（自带 7d/30d/12m/All 窗口）、平台占比、模型性能矩阵、请求日志。支持全屏与暂停刷新</em></p>

<details>
<summary>📱 点击展开其余页面截图</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="流量" />
</p>
<p align="center"><em>流量 — 顶部「需要关注」汇总限流/失败/无流量三类异常；请求表可按模型、平台、密钥、状态、错误类型、延时区间筛选并分页回溯；右侧单笔详情含尝试链（每次后端尝试的状态与耗时）、TTFT / 生成耗时条、缓存读与新增输入、预估成本，并可复制 ID、重放请求</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="虚拟模型" />
</p>
<p align="center"><em>虚拟模型 — 列表直接给出健康状态、后端数、负载策略、请求量、成功率、平均延时与成本</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="虚拟模型路由" />
</p>
<p align="center"><em>模型详情 · 路由 — 左侧改策略 / 重试次数 / 退避 / 默认 max_tokens / 权重分布，中间「路由脊柱」把后端按权重与优先级画成连线图，右侧看单个后端的供应商、能力标记与思考参数处理方式</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="平台" />
</p>
<p align="center"><em>平台 — 连通性检测结果、已同步模型数、被多少虚拟模型引用、请求量与成功率；顶栏会汇总「N degraded」</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="平台详情" />
</p>
<p align="center"><em>平台详情 — 凭据与转发头（Key 默认打码，点击才显一次）、测试连接即拉取远程模型、模型行与定价 CSV 批量导入导出、以及「哪些虚拟模型在用这个平台」</em></p>

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
<p align="center"><em>Playground — 选虚拟模型 + 协议（OpenAI / Anthropic）发一笔真实请求，用来验证路由与上游；可就地生成一次性密钥并填好，或复制等价 cURL</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="设置" />
</p>
<p align="center"><em>设置 — 服务端口与监听地址、请求策略默认值、账号安全、数据统计（行数 / 尝试轨迹 / 告警 / 悬空行 / 正文留存）、配置导出导入、整库备份与恢复</em></p>

</details>

---

## ⚡ 核心能力

### 🎯 多 Key 负载均衡 — 限流瓶颈？不存在的

多个 API Key 轮询/加权/分流，**N 个 Key = N 倍吞吐量**：

```
你的应用（高频请求）
    │
    └──→ AI Gateway 智能负载均衡 ──┬── Key 1: sk-free-xxx1（3 RPM）
                                    ├── Key 2: sk-free-xxx2（3 RPM）
                                    ├── Key 3: sk-free-xxx3（3 RPM）
                                    └── Key 4: sk-free-xxx4（3 RPM）
                                        ↓
                            总吞吐量：4 × 3 = 12 RPM 🚀
```

> 💡 **白嫖党福音**：SiliconFlow、Groq 等平台免费额度 × 多账号 = 免费大模型无限用

### 🔀 5 种负载均衡策略

| 策略 | 一句话 | 最适合 |
|------|--------|--------|
| **轮询** | 一个接一个，雨露均沾 | 多 Key 突破限流 |
| **加权随机** | 权重大的多干活 | 后端性能不一 |
| **最少连接** | 谁闲谁接活 | 流式输出场景 |
| **优先级** | 便宜的先上，贵的兜底 | 省钱！DeepSeek → GPT 兜底 |
| **延迟优先** | 谁快选谁 | 在线服务对延迟敏感 |

### 🌐 OpenAI & Anthropic 双协议 — 一个入口全搞定

```
OpenAI SDK ──→ /v1/chat/completions ──┐
                                       ├──→ AI Gateway ──→ DeepSeek / Qwen / GPT / Claude ...
Anthropic SDK ──→ /v1/messages ───────┘
```

**零代码改造**，把 `base_url` 换成 `http://localhost:1994/v1` 就完事了。

### 🧠 思考模式 — 客户端说了算

客户端传什么思考参数，网关就翻译成上游认识的格式；客户端不传，网关不主动开启。

| 客户端传入 | 上游是 Claude | 上游是 OpenAI 兼容推理模型 |
|---|---|---|
| `reasoning_effort: high`（OpenAI） | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: high` |
| `thinking: {budget_tokens: 10000}`（Anthropic） | 原样保留，`max_tokens` 不足时压缩 budget | `reasoning_effort: medium` |

- 后端可配置**思考参数风格**：`Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / 禁用
- 支持 Anthropic `thinking` / `redacted_thinking` 块的完整回放与跨协议签名缓存，多轮工具调用不会因缺少签名被上游拒绝
- `/v1/models` 额外返回 `capabilities` / `supported_parameters` / `reasoning`，读取这些字段的客户端可自动展示思考档位
- 注意：Qoder / Trae / WorkBuddy 等客户端是否展示“思考程度”开关，仍取决于其内置的模型能力表，不读取上述扩展字段

### 🛡️ 高可用 · 不怕单点故障

- 后端挂了？**自动切换**到健康的后端
- 被限流了？429 / 5xx / 超时 **自动重试** + 指数退避
- 完全透明：你的应用感知不到后端切换

### 📊 监控大屏 · 全链路性能统计

每个模型、每个时段（时/日/月/年）的完整性能画像：

- **8 项指标趋势**：请求 / 失败 / 输入 / 输出 / 延时 / 首字时长（TTFT）/ 输出速度（tok/s）/ 缓存命中
- **模型性能矩阵**：一屏看清所有模型的成功率、耗时、速度、缓存命中率，跟随指标自动排序
- **请求明细**：逐条记录输入 / 新增输入 / 输出 / 缓存读 / 耗时 / 首字 / 速度，支持模型与状态筛选、分页回溯
- **缓存命中率**（CC Switch 口径）：`命中 / (命中 + 新增输入)`，直观反映前缀缓存的真实收益

---

## 🆚 和其他方案的区别

| | AI Gateway | Nginx 反代 | One API | LiteLLM |
|---|---|---|---|---|
| **开箱即用** | ✅ 桌面 App + 服务器 | ❌ 需要写配置 | ✅ | ✅ |
| **可视化管理** | ✅ Web UI | ❌ | ✅ | ❌ |
| **负载均衡** | ✅ 5 种策略 | ✅ 有限 | ⚠️ 简单 | ✅ |
| **OpenAI 协议** | ✅ | ✅ | ✅ | ✅ |
| **Anthropic 协议** | ✅ 原生支持 | ❌ 需额外配置 | ✅ 转换 | ✅ 转换 |
| **自动故障切换** | ✅ | ⚠️ 需配置 | ⚠️ | ⚠️ |
| **远程模型获取** | ✅ 自动拉取 | ❌ | ❌ | ❌ |
| **本地部署** | ✅ 单二进制零依赖 | ✅ | ✅ Docker | ✅ pip |
| **桌面应用** | ✅ macOS/Win/Linux | ❌ | ❌ | ❌ |
| **语言** | Rust（高性能低内存） | C | Go | Python |

---

## ✨ 更多特性

- 🏪 **15 个平台预设**：OpenAI · Anthropic · DeepSeek · Qwen · Google Gemini · Groq · SiliconFlow · 智谱 GLM · 月之暗面 · 豆包 · NVIDIA · Together AI · OpenRouter · Ollama · Azure OpenAI ··· 一键添加，也可手工填任意 OpenAI / Anthropic 兼容端点
- 🔍 **远程模型获取**：测试连接即拉取上游模型列表，勾选同步进本地模型表；能力标记（推理 / 视觉 / 函数调用等）随模型维护
- 🧠 **智能能力识别**：聊天/代码/视觉/函数调用等能力自动填充
- 🔗 **reasoning_content 支持**：兼容 NVIDIA NIM 等平台思维链输出；OpenAI / Anthropic 思考参数双向翻译
- 📡 **自定义转发头**：在平台级追加任意请求头（如供应商私有开关），同名可覆盖 Authorization / x-api-key
- 🎛️ **Playground**：界内选模型、选协议、发真实请求，验证路由与上游是否如预期，并可复制等价 cURL
- 🌙 **浅色 / 深色 / 跟随系统** 三种主题
- 🌍 **中英双语** UI
- ⚙️ 端口可在界面内修改，默认 1994
- 🧭 **七个页面**：流量 · 监控大屏 · 虚拟模型 · 平台 · API 密钥 · Playground · 设置。筛选、分页与时段都写进地址，刷新、前进/后退都不丢
- 🧾 **请求轨迹与正文重放**：一次请求的每次后端尝试逐条可查；正文留存默认关闭，开启后可原样重放某一笔请求
- 🔔 **通知中心**：Key 即将过期、平台连续失败、配额超阈值、备份过期都会落库，顶栏铃铛一点就到
- 💰 **成本估算与预算配额**：按定价表算出每次请求的成本（并给出覆盖率），配额支持滚动窗口与超额拦截
- 🔑 **密钥级配额**：每把 Key 可设到期时间、请求数上限、Token 上限、成本预算与配额窗口（终身 / 日 / 周 / 月），并可绑定到单个虚拟模型
- 🗄️ **整库备份与恢复**：一键快照、下载、删除，原地还原前自动再留一份安全快照
- 📤 **配置导出 / 导入与 CSV**：配置脱敏导出、预览后合并导入；流量按筛选导出 CSV，平台定价 CSV 批量进出
- ⚡ **实时刷新**：新请求与告警通过 SSE 推送，连接断开自动退回 30 秒轮询

---

## 🔐 登录鉴权

管理后台（平台配置、虚拟大模型、API Key、设置、概览与统计）需要登录后才能访问：

- **首次启动**：打开管理页面会进入「初始化管理员账号」，设置用户名和密码后自动登录。为避免局域网内被他人抢先注册，首次初始化仅允许从本机（127.0.0.1）发起。
- **会话**：登录状态保存在 HttpOnly Cookie 中，默认 7 天有效；右上角用户菜单可退出登录。
- **修改密码**：设置 → 账号安全。修改后其他设备上的登录会话会立即失效。
- **忘记密码**：停止应用，删除数据库中 `users` 与 `sessions` 两张表的记录后重启，会重新进入初始化流程。

访问控制范围：

| 路径 | 鉴权方式 |
|---|---|
| `/v1/chat/completions`、`/v1/completions`、`/v1/messages` | 网关 API Key（`Authorization: Bearer ...` 或 `x-api-key`） |
| `/v1/models` | 公开 |
| `/health` | 公开 |
| `/api/auth/status`、`/api/auth/login`、`/api/auth/setup` | 公开 |
| 其余 `/api/*`（平台 / 模型 / 虚拟大模型 / API Key / 设置 / 统计） | 管理员登录会话 |

> 尚未创建任何 API Key 时网关会放行 AI 请求（兼容旧配置）；创建 API Key 后必须携带有效 Key。账号密码保存在 SQLite 中（Argon2 哈希），`config.toml` 的 `admin_token` 为历史保留字段。

---

## 🚀 快速开始

### 方式一：下载桌面应用（推荐）

前往 [Releases](https://github.com/keiskeies/ai-gateway/releases) 下载对应平台安装包，双击即用。

### 方式二：从源码构建

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# 独立服务器模式
cargo run
# 访问 http://localhost:1994

# 或 Tauri 桌面应用模式
cargo install tauri-cli
cargo tauri dev
```

### 三步上手

```
0️⃣ 初始化管理员账号 → 登录管理后台
1️⃣ 添加平台 → 选预设 + 填 API Key → 保存
2️⃣ 创建虚拟模型 → 选策略 + 添加后端 → 启动
3️⃣ 改 base_url → 指向 http://localhost:1994/v1 → 完事
```

---

## 📖 调用示例

改一行代码就能接入，**无需其他任何修改**：

```python
# OpenAI SDK — 只改 base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← 就改这一行
    api_key="your-token"
)
response = client.chat.completions.create(
    model="your-virtual-model",  # ← 填你创建的虚拟模型名
    messages=[{"role": "user", "content": "hello"}]
)

# Anthropic SDK — 同样只改 base_url
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",  # ← 就改这一行
    api_key="your-token"
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
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic 兼容格式
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# 模型列表
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
```

</details>

---

## 🎯 谁在用？典型场景

| 场景 | 怎么用 | 收益 |
|------|--------|------|
| **白嫖党** | 多个免费 Key 轮询 | 免费额度 × N 倍 |
| **省钱党** | DeepSeek 优先 + GPT 兜底 | 成本降 80%+ |
| **稳定性党** | 多后端 + 自动故障切换 | 可用性 99.9% |
| **多平台党** | OpenAI + Claude + Qwen 统一入口 | 一行代码切换 |
| **团队党** | 统一网关 + API Key 管理 | 安全 + 可控 |

---

## 🏗️ 架构

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │  兼容端点 │  │  兼容端点  │  │  管理界面    │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │    路由 & 负载均衡引擎    │                     │
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

编辑 `config.toml`（首次运行自动生成）：

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "your-secret-key"
admin_token = ""

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
```

> 💡 管理端口也可在桌面应用的「设置」页面中直接修改，无需手动编辑配置文件。

---

## 🛠️ 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust · Actix-Web · SQLite (r2d2) · Reqwest |
| 前端 | React · TypeScript · Mantine · Vite |
| 桌面端 | Tauri 2.0 |
| 数据库 | SQLite (rusqlite + r2d2 连接池) |

---

## 🗺️ Roadmap

- [x] 📊 更丰富的统计面板（监控大屏：每模型全时段趋势 / 请求明细 / 缓存命中率）
- [x] 🔑 API Key 细粒度权限控制（绑定虚拟模型 · 到期时间 · 请求/Token/成本三类配额 · 滚动窗口拦截）
- [x] 🔔 后端健康检查 & 异常告警通知（平台连通性检测 + 通知中心落库）
- [ ] 🌐 多语言网关（翻译 / TTS / Embedding 统一端点）
- [ ] 🐳 Docker 一键部署
- [ ] ☁️ 云端配置同步

> 有想要的功能？欢迎 [提 Issue](https://github.com/keiskeies/ai-gateway/issues)！

---

## 🤝 参与贡献

欢迎各种形式的贡献！

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 发起 Pull Request

---

## 📂 项目结构

```
ai-gateway/
├── src/                  # Rust 后端
│   ├── api/              # REST API
│   ├── db/               # 数据库层 (r2d2 连接池)
│   ├── proxy/            # 代理处理器
│   ├── lb/               # 负载均衡引擎
│   ├── protocol/         # OpenAI/Anthropic 协议适配
│   └── models/           # 数据模型
├── frontend/             # React 前端
│   └── src/
│       ├── i18n.ts       # 国际化
│       ├── presets.ts    # 平台/模型预设
│       └── pages/        # 页面组件
├── src-tauri/            # Tauri 桌面应用
├── doc/                  # 截图 & 文档
├── config.toml           # 配置文件
└── data/                 # SQLite 数据库
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
  觉得有用？给个 <a href="https://github.com/keiskeies/ai-gateway">⭐ Star</a> 支持一下呗！
</p>
