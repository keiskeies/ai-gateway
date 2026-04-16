<p align="center">
  <img src="logo.png" width="128" height="128" alt="AI Gateway Logo" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?logo=rust" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?logo=typescript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Cross-Platform AI API Aggregation & Intelligent Traffic Load Balancer</strong><br/>
  Unified access to OpenAI · Anthropic · Google Gemini · DeepSeek · Qwen · Moonshot · ZhipuAI · Doubao · Ollama · NVIDIA NIM · Azure · and more...<br/><br/>
  <strong>Dual-Protocol Native Support:</strong> Compatible with both OpenAI and Anthropic Messages API formats — one gateway for the entire ecosystem
</p>

<p align="center">
  <a href="README.md">中文</a> | <a href="README_EN.md">English</a>
</p>

---

## 🌟 Dual-Protocol Native Support — OpenAI & Anthropic

AI Gateway is more than an OpenAI-compatible proxy — it **natively supports the Anthropic Messages API protocol**, with zero conversion middleware:

```
Your Application
    │
    ├── Using OpenAI SDK ────→ POST /v1/chat/completions ──┐
    │                                                       │
    └── Using Anthropic SDK ─→ POST /v1/messages ──────────┤
                                                            │
                                                     AI Gateway
                                                            │
                                          ┌─────────────────┼─────────────────┐
                                          ↓                 ↓                 ↓
                                      DeepSeek            Qwen            OpenAI
                                     (weight 3)        (weight 2)       (weight 1)
```

**What this means:**
- 🔄 **OpenAI SDK users**: Point `base_url` to Gateway, zero code changes
- 🔄 **Anthropic SDK users**: Same — supports `x-api-key` auth, `messages` format, streaming
- 🔄 **Mixed usage**: One Gateway serves both protocols, backends can be any platform's models
- 💡 **Anthropic passthrough**: When using Anthropic backends, requests forward natively (tool_use, thinking, etc.)

---

## ✨ Key Highlights

### 🔀 AI API Traffic Load Balancing

| Strategy | Description | Use Case |
|----------|-------------|----------|
| **Round Robin** | Sequential distribution across backends | Equal-capacity backends |
| **Weighted Random** | Weighted random distribution | Heterogeneous backends |
| **Least Connections** | Prefer backend with fewest active connections | Streaming scenarios |
| **Priority** | Active-standby, auto-failover | Cost optimization: cheap first, expensive as fallback |
| **Latency Based** | Prefer fastest responding backend | Latency-sensitive services |

- **High Availability**: Automatic failover when a backend goes down
- **Cost Optimization**: Prioritize cost-effective models, expensive ones as fallback
- **Zero Code Change**: Point your API Base URL to Gateway — no client modification needed
- **Dual-Protocol**: Native support for both OpenAI and Anthropic API formats

### 🌐 More Features

- **One-Click Platform Setup**: 15+ built-in AI platform presets (including Google Gemini)
- **Model Presets**: Auto-fill popular model IDs
- **Dark/Light/System Theme**: Three modes
- **i18n**: Full Chinese and English support
- **Configurable Port**: Admin port adjustable in UI, default 1994
- **Cross-Platform Desktop**: macOS / Windows / Linux (Tauri)
- **Standalone Server**: Single binary, zero dependencies

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────┐
│                  AI Gateway                   │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ OpenAI  │  │Anthropic│  │  Admin Web   │ │
│  │Endpoint │  │Endpoint │  │     UI       │ │
│  └────┬────┘  └────┬────┘  └──────────────┘ │
│  ┌────▼────────────▼────┐                    │
│  │  Routing & LB Engine │                    │
│  └────┬───┬───┬────┬────┘                    │
│  ┌────▼┐ ┌▼──┐┌▼───┐┌▼────┐                │
│  │Deep │ │Qwen││GLM ││GPT │  ← Multi-backend│
│  │Seek │ │    ││    ││-4o │                │
│  └─────┘ └───┘└────┘└─────┘                │
└──────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### Option 1: Standalone Server

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway
cargo run
# Open http://localhost:1994
```

### Option 2: Tauri Desktop App

```bash
cargo install tauri-cli
cargo tauri dev      # Development
cargo tauri build    # Build desktop app
```

### Option 3: Frontend Development

```bash
# Terminal 1
cargo run
# Terminal 2
cd frontend && npm install && npm run dev
```

---

## ⚙️ Configuration

Edit `config.toml` (auto-generated on first run):

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

> 💡 The admin port can also be changed in the desktop app's Settings page — no need to edit config files manually.

---

## 📖 Usage Guide

### 1️⃣ Add Platform → 2️⃣ Add Model → 3️⃣ Create Proxy → 4️⃣ Configure Routes → 5️⃣ Call API

```bash
# OpenAI compatible format (works with any OpenAI SDK)
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic compatible format (works with any Anthropic SDK)
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# List models
curl http://localhost:1994/v1/models -H "Authorization: Bearer YOUR_TOKEN"
```

**Python Example:**

```python
# OpenAI SDK
from openai import OpenAI
client = OpenAI(base_url="http://localhost:1994/v1", api_key="YOUR_TOKEN")
response = client.chat.completions.create(
    model="your-virtual-model",
    messages=[{"role": "user", "content": "hello"}]
)

# Anthropic SDK
import anthropic
client = anthropic.Anthropic(base_url="http://localhost:1994", api_key="YOUR_TOKEN")
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust · Actix-Web · SQLite (r2d2) · Reqwest |
| Frontend | React · TypeScript · Ant Design · Vite |
| Desktop | Tauri 2.0 |

---

## 📜 License

MIT License
