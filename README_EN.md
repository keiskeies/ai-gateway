<p align="center">
  <img src="logo.png" width="120" alt="AI Gateway Logo" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Cross-platform AI API Aggregation & Intelligent Load Balancing</strong><br/>
  Unified access to OpenAI · Anthropic · Google Gemini · DeepSeek · Qwen · Moonshot · ZhipuAI · Doubao · Ollama · NVIDIA NIM · Azure · More...<br/><br/>
  <strong>Dual-Protocol Native Support:</strong> Compatible with both OpenAI and Anthropic Messages API formats — one gateway covers the entire ecosystem
</p>

<p align="center">
  <a href="README.md">中文</a> | <a href="README_EN.md">English</a>
</p>

---

## 🔥 Why AI Gateway?

### 💡 Multi-Key Load Balancing — Break Through AI Rate Limits

Many AI platforms offer free tiers but impose strict rate limits (e.g., 3 or 10 requests per minute). AI Gateway's core capability is **distributing requests across multiple API keys via load balancing**, effectively multiplying your rate limit by N:

```
Your App (High-frequency requests)
    │
    └──→ AI Gateway Load Balancer ──┬── Key 1: sk-free-xxx1 (3 RPM)
                                      ├── Key 2: sk-free-xxx2 (3 RPM)
                                      ├── Key 3: sk-free-xxx3 (3 RPM)
                                      └── Key 4: sk-free-xxx4 (3 RPM)
                                            ↓
                                Total throughput: 4 × 3 = 12 RPM 🚀
```

**It's super simple to set up**:
1. In "Platforms", add the same AI platform multiple times with different API keys
2. In "Virtual Models", add models from each key as backends
3. Choose a load balancing strategy and start — requests are automatically distributed

> Not just for free tiers! Even with paid platforms, multi-key load balancing significantly improves concurrent throughput and avoids single-key rate limit failures.

### 🔀 5 Intelligent Load Balancing Strategies

| Strategy | Description | Best For |
|----------|-------------|----------|
| **Round Robin** | Distributes requests sequentially across backends | Multi-key rate limit breaking: evenly spread requests across keys |
| **Weighted Random** | Random distribution weighted by backend capacity | Mixed-performance backends |
| **Least Connections** | Prefers the backend with fewest active connections | Long-running/streaming scenarios |
| **Priority** | Failover mode — uses highest-priority backend first, auto-switches on failure | Cost optimization: cheap keys first, expensive keys as fallback |
| **Latency Based** | Tracks response latency in real-time, selects fastest backend | Latency-sensitive online services |

### 🌐 Dual-Protocol Native Support

AI Gateway is more than an OpenAI-compatible proxy — it **natively supports the Anthropic Messages API protocol** with no conversion middleware needed:

```
Your Application Code
    │
    ├── Using OpenAI SDK ──────→ POST /v1/chat/completions ──┐
    │                                                       │
    └── Using Anthropic SDK ──→ POST /v1/messages ───────────┤
                                                            │
                                                      AI Gateway
                                                            │
                                          ┌─────────────────┼─────────────────┐
                                          ↓                 ↓                 ↓
                                      DeepSeek            Qwen            OpenAI
                                     (weight 3)         (weight 2)       (weight 1)
```

### 🛡️ High Availability & Smart Retries

- **Automatic failover**: When a backend goes down, traffic is automatically routed to healthy backends
- **Smart retries**: 429 rate limiting, 5xx server errors, and timeouts are automatically retried with exponential backoff
- **Zero code changes**: Just point your API Base URL to AI Gateway — no modifications needed

---

## ✨ More Features

- **One-click platform setup**: 15+ built-in AI platform presets (including Google Gemini)
- **Model presets**: Popular model IDs auto-filled, no manual lookup needed
- **Day/Night mode**: Light, dark, and system-follow modes
- **Bilingual**: Full i18n support — switch between Chinese and English
- **Configurable port**: Admin port can be changed in the UI (default: 1994)
- **Cross-platform desktop app**: Native support for macOS / Windows / Linux (via Tauri)
- **Standalone deployment**: Single binary, zero dependencies — perfect for servers

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────┐
│                AI Gateway                     │
│                                              │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ OpenAI  │  │Anthropic│  │  Admin Web   │ │
│  │ Endpoint│  │ Endpoint│  │  Dashboard   │ │
│  └────┬────┘  └────┬────┘  └──────────────┘ │
│       │            │                         │
│  ┌────▼────────────▼────┐                    │
│  │  Routing & LB Engine │                    │
│  └────┬───┬───┬────┬────┘                    │
│       │   │   │    │                         │
│  ┌────▼┐ ┌▼──┐┌▼───┐┌▼────┐                │
│  │Deep │ │Qwen││GLM ││GPT │  ← Multi-backend│
│  │Seek │ │    ││    ││-4o │                  │
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
cargo tauri build    # Production build
```

### Option 3: Frontend Dev Mode

```bash
# Terminal 1: Start backend
cargo run

# Terminal 2: Start frontend dev server
cd frontend
npm install
npm run dev
```

---

## 🎯 Typical Use Cases

### Use Case 1: Multi-Key Rate Limit Breaking

> Platforms like SiliconFlow and Groq offer free tiers with severe rate limits (3–30 RPM). Register multiple accounts for multiple keys, then use AI Gateway's load balancing to distribute requests — total throughput scales by N.

1. In "Platforms", add the same platform multiple times with different API keys
2. Add the same models under each platform
3. Create a virtual model with all backends, choose "Round Robin"
4. Start — keys rotate automatically, rate limit increases N×

### Use Case 2: Cost Optimization — Cheap First, Premium Fallback

> DeepSeek is extremely cheap but occasionally unstable; OpenAI is reliable but expensive. Use "Priority" strategy — DeepSeek first, auto-fallback to OpenAI on failure.

### Use Case 3: Unified Multi-Platform Entry Point

> Your app needs both GPT-4o and Claude. AI Gateway supports both protocols simultaneously — one gateway covers everything.

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

---

## 📖 Usage Guide

### 1️⃣ Add a Platform

Navigate to "Platforms" → Click "Add Platform" → Select a preset or custom → Enter API Key → Save

### 2️⃣ Add Models

Navigate to "Models" → Click "Add Model" → Select platform → Choose preset or type model ID → Save

### 3️⃣ Create an Aggregated API

Navigate to "Aggregated APIs" → Click "New API" → Set name and port → Select protocol (OpenAI / Anthropic)

### 4️⃣ Configure Virtual Models

In API details → Click "Add" → Set virtual model name → Choose load balancing strategy → Add backend models

### 5️⃣ Call the API

```bash
# OpenAI compatible format (works with all OpenAI SDKs)
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic compatible format (works with all Anthropic SDKs)
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# List models
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
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
| Database | SQLite (rusqlite + r2d2 connection pool) |

---

## 📂 Project Structure

```
ai-gateway/
├── src/                  # Rust backend
│   ├── lib.rs            # Library entry
│   ├── main.rs           # Standalone server entry
│   ├── config.rs         # Configuration management
│   ├── api/              # REST API
│   ├── db/               # Database layer (r2d2 pool)
│   ├── proxy/            # Proxy handlers
│   ├── lb/               # Load balancing engine
│   ├── protocol/         # OpenAI/Anthropic protocol adapter
│   └── models/           # Data models
├── frontend/             # React frontend
│   └── src/
│       ├── i18n.ts       # Internationalization
│       ├── presets.ts    # Platform/model presets
│       ├── ThemeContext.tsx # Theme management
│       └── pages/        # Page components
├── src-tauri/            # Tauri desktop app
├── static/               # Build output (frontend)
├── config.toml           # Configuration file
└── data/                 # SQLite database
```

---

## 📜 License

MIT License
