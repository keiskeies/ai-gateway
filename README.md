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

<p align="center">
  <img src="doc/ai-gateway1.jpg" width="80%" alt="AI Gateway 主界面" />
</p>
<p align="center"><em>主界面 — 虚拟大模型管理</em></p>

<details>
<summary>📱 点击查看更多截图</summary>

<p align="center">
  <img src="doc/ai-gateway2.jpg" width="80%" alt="AI Gateway 平台管理" />
</p>
<p align="center"><em>平台管理 — 添加与配置 AI 平台</em></p>

<p align="center">
  <img src="doc/a-gateway-1.jpg" width="80%" alt="AI Gateway 虚拟模型配置" />
</p>
<p align="center"><em>虚拟模型 — 后端配置与负载均衡策略</em></p>

<p align="center">
  <img src="doc/ai-gateway21.jpg" width="80%" alt="AI Gateway 统计概览" />
</p>
<p align="center"><em>统计概览 — Token 用量与请求统计</em></p>

<p align="center">
  <img src="doc/ai-gateway3.jpg" width="80%" alt="AI Gateway 设置页面" />
</p>
<p align="center"><em>设置页面 — 端口配置与主题切换</em></p>

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

### 🛡️ 高可用 · 不怕单点故障

- 后端挂了？**自动切换**到健康的后端
- 被限流了？429 / 5xx / 超时 **自动重试** + 指数退避
- 完全透明：你的应用感知不到后端切换

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

- 🏪 **15+ 平台预设**：OpenAI / Anthropic / DeepSeek / Qwen / Gemini / GLM / 月之暗面 / 豆包 / Ollama / NVIDIA NIM / Azure / 云硅 / Groq / 零一万物 / 百川 ··· 一键添加
- 🔍 **远程模型获取**：选择平台后自动拉取可用模型列表，下拉选择无需手动输入
- 🧠 **智能能力识别**：聊天/代码/视觉/函数调用等能力自动填充
- 🔗 **reasoning_content 支持**：兼容 NVIDIA NIM 等平台思维链输出
- 🌙 **浅色 / 深色 / 跟随系统** 三种主题
- 🌍 **中英双语** UI
- ⚙️ 端口可在界面内修改，默认 1994

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
| 前端 | React · TypeScript · Ant Design · Vite |
| 桌面端 | Tauri 2.0 |
| 数据库 | SQLite (rusqlite + r2d2 连接池) |

---

## 🗺️ Roadmap

- [ ] 📊 更丰富的统计面板（按天/周/月的用量趋势图）
- [ ] 🔑 API Key 细粒度权限控制（按模型/按额度限流）
- [ ] 🌐 多语言网关（翻译 / TTS / Embedding 统一端点）
- [ ] 🔔 后端健康检查 & 异常告警通知
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
