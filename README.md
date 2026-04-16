<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>跨平台 AI 接口聚合与智能流量负载均衡工具</strong><br/>
  统一接入 OpenAI · Anthropic · DeepSeek · Qwen · 月之暗面 · 智谱AI · 豆包 · Ollama · NVIDIA NIM · Azure · 更多...
</p>

---

## ✨ 核心亮点

### 🔀 AI 接口流量负载均衡

AI Gateway 的核心能力——**将多个 AI 后端聚合为统一入口，智能分配请求流量**：

| 策略 | 说明 | 适用场景 |
|------|------|----------|
| **轮询 (Round Robin)** | 依次将请求分配到各后端，循环往复 | 后端能力均等，均匀分摊流量 |
| **加权随机 (Weighted Random)** | 按权重随机分配，高权重后端获得更多请求 | 后端性能差异大，按配额分配 |
| **最少连接 (Least Connections)** | 优先选择当前活跃连接数最少的后端 | 长连接/流式场景，避免单点过载 |
| **优先级 (Priority)** | 主备模式，高优先级后端优先，故障自动切换 | 成本优化：便宜模型优先，昂贵模型兜底 |
| **延迟优先 (Latency Based)** | 实时追踪各后端响应延迟，优先选择最快后端 | 对延迟敏感的在线服务 |

**典型场景：**

```
用户请求 → AI Gateway → 路由匹配 → 负载均衡 → 后端选择 → 转发请求 → 流式回传
              ↓
    ┌─────────┼─────────┐
    ↓         ↓         ↓
 DeepSeek   Qwen    OpenAI   ← 同一虚拟模型名，多后端负载均衡
  (权重3)   (权重2)   (权重1)   ← 加权随机策略
```

- **高可用**：某个后端宕机，流量自动切换到健康后端
- **成本优化**：优先使用性价比高的模型，贵模型只做兜底
- **零代码改造**：客户端只需将 API Base URL 指向 Gateway，无需任何修改
- **多协议兼容**：同时支持 OpenAI 和 Anthropic API 格式

### 🌐 其他特性

- **一键添加平台**：内置 12+ 主流 AI 平台预设，点击即用
- **模型预设**：热门模型 ID 自动填充，无需手动查找
- **日/夜间模式**：支持浅色、深色、跟随系统三种模式
- **中英双语**：完整国际化支持，一键切换语言
- **跨平台桌面应用**：macOS / Windows / Linux 原生支持（基于 Tauri）
- **也可独立部署**：单二进制文件，零依赖运行，适合服务器部署

---

## 🏗️ 架构

```
┌──────────────────────────────────────────────┐
│                  AI Gateway                   │
│                                              │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ OpenAI  │  │Anthropic│  │  Admin Web   │ │
│  │ 兼容端点 │  │ 兼容端点 │  │  管理界面    │ │
│  └────┬────┘  └────┬────┘  └──────────────┘ │
│       │            │                         │
│  ┌────▼────────────▼────┐                    │
│  │   路由 & 负载均衡引擎  │                    │
│  └────┬───┬───┬────┬────┘                    │
│       │   │   │    │                         │
│  ┌────▼┐ ┌▼──┐┌▼───┐┌▼────┐                │
│  │Deep │ │Qwen││GLM ││GPT │  ← 多后端       │
│  │Seek │ │    ││    ││-4o │                │
│  └─────┘ └───┘└────┘└─────┘                │
└──────────────────────────────────────────────┘
```

---

## 🚀 快速开始

### 方式一：独立服务器模式

```bash
# 克隆仓库
git clone https://github.com/your-username/ai-gateway.git
cd ai-gateway

# 编译运行
cargo run

# 访问管理界面
open http://localhost:1994
```

### 方式二：Tauri 桌面应用模式

```bash
# 安装 Tauri CLI
cargo install tauri-cli

# 开发模式
cargo tauri dev

# 构建桌面应用
cargo tauri build
```

### 方式三：前端开发模式

```bash
# 终端 1：启动后端
cargo run

# 终端 2：启动前端开发服务器
cd frontend
npm install
npm run dev
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

---

## 📖 使用指南

### 1️⃣ 添加平台

进入「平台管理」→ 点击「添加平台」→ 选择预设平台或自定义 → 填写 API Key → 保存

### 2️⃣ 添加模型

进入「模型管理」→ 点击「添加模型」→ 选择平台 → 从预设选择或手动输入模型 ID → 保存

### 3️⃣ 创建代理

进入「代理管理」→ 点击「新建代理」→ 设置名称和端口 → 选择协议（OpenAI/Anthropic）

### 4️⃣ 配置路由

在代理详情中 → 点击「添加路由」→ 设置虚拟模型名 → 选择负载均衡策略 → 添加后端模型

### 5️⃣ 调用 API

```bash
# OpenAI 兼容格式
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic 兼容格式
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# 模型列表
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
```

---

## 🛠️ 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust · Actix-Web · SQLite (r2d2) · Reqwest |
| 前端 | React · TypeScript · Ant Design · Vite |
| 桌面端 | Tauri 2.0 |
| 数据库 | SQLite (rusqlite + r2d2 连接池) |

---

## 📂 项目结构

```
ai-gateway/
├── src/                  # Rust 后端
│   ├── lib.rs            # 库入口
│   ├── main.rs           # 独立服务器入口
│   ├── config.rs         # 配置管理
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
│       ├── ThemeContext.tsx # 主题管理
│       └── pages/        # 页面组件
├── src-tauri/            # Tauri 桌面应用
├── static/               # 构建产物（前端）
├── config.toml           # 配置文件
└── data/                 # SQLite 数据库
```

---

## 📜 License

MIT License
