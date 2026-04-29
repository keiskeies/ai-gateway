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
  <strong>Cross-platform AI API Aggregation & Intelligent Load Balancer</strong><br/>
  Unified access to OpenAI · Anthropic · Google Gemini · DeepSeek · Qwen · Moonshot · Zhipu AI · Doubao · Ollama · NVIDIA NIM · Azure · More...<br/><br/>
  <strong>Dual-Protocol Native Support:</strong> Compatible with both OpenAI and Anthropic Messages API formats — one gateway for the entire ecosystem
</p>

<p align="center">
  <a href="README.md">中文</a> | <a href="README_EN.md">English</a>
</p>

---

## 📸 Screenshots

<p align="center">
  <img src="doc/ai-gateway1.jpg" width="80%" alt="AI Gateway Main Interface" />
</p>
<p align="center"><em>Main Interface — Virtual Model Management</em></p>

<p align="center">
  <img src="doc/a-gateway-1.jpg" width="80%" alt="AI Gateway Platform Management" />
</p>
<p align="center"><em>Platform Management — Add & Configure AI Platforms</em></p>

<p align="center">
  <img src="doc/ai-gateway21.jpg" width="80%" alt="AI Gateway Virtual Model Configuration" />
</p>
<p align="center"><em>Virtual Models — Backend Configuration & Load Balancing Strategies</em></p>

<p align="center">
  <img src="doc/ai-gateway3.jpg" width="80%" alt="AI Gateway Settings & Statistics" />
</p>
<p align="center"><em>Settings Page — Port Configuration & Data Statistics</em></p>

---

## 🔥 Why AI Gateway?

### 💡 Multi-Key Load Balancing — Break Through AI Platform Rate Limits

Many AI platforms offer free tiers but impose strict rate limits (e.g., 3 or 10 requests per minute). AI Gateway's core capability is **distributing requests across multiple API Keys via load balancing**, effectively multiplying your rate limit by N:

```
Your App (high-frequency requests)
    │
    └──→ AI Gateway Smart Load Balancer ──┬── Key 1: sk-free-xxx1 (3 RPM)
                                           ├── Key 2: sk-free-xxx2 (3 RPM)
                                           ├── Key 3: sk-free-xxx3 (3 RPM)
                                           └── Key 4: sk-free-xxx4 (3 RPM)
                                               ↓
                                   Total throughput: 4 × 3 = 12 RPM 🚀
```

**Super simple setup**:
1. In "Platform Management", add the same AI platform multiple times with different API Keys
2. In "Virtual Models", add models from these same-platform-different-key configurations as backends
3. Select a load balancing strategy and start — requests are automatically distributed across keys

> Not just for free tiers! Even with paid platforms, multi-key load balancing significantly boosts concurrent throughput and prevents single-key rate-limit failures.

### 🔀 5 Intelligent Load Balancing Strategies

| Strategy | Description | Best For |
|----------|-------------|----------|
| **Round Robin** | Distributes requests to backends in sequential order | Multi-key rate limit bypass: evenly spread requests across keys |
| **Weighted Random** | Randomly assigns based on weights; higher-weight backends get more traffic | Backends with varying performance; quota-based allocation |
| **Least Connections** | Prioritizes backends with the fewest active connections | Long-lived/streaming connections; avoid single-point overload |
| **Priority** | Active-standby mode; higher-priority backends first, auto-failover on error | Cost optimization: cheap keys first, expensive keys as fallback |
| **Latency Based** | Tracks real-time response latency per backend; selects the fastest | Latency-sensitive online services |

### 🌐 Dual-Protocol Native Support

AI Gateway is more than just an OpenAI-compatible proxy — **it natively supports the Anthropic Messages API protocol** without any protocol conversion middleware:

```
Your Application Code
    │
    ├── Using OpenAI SDK ─────→ POST /v1/chat/completions ──┐
    │                                                        │
    └── Using Anthropic SDK ──→ POST /v1/messages ──────────┤
                                                             │
                                                       AI Gateway
                                                             │
                                          ┌──────────────────┼──────────────────┐
                                          ↓                  ↓                  ↓
                                      DeepSeek            Qwen             OpenAI
                                     (weight 3)        (weight 2)        (weight 1)
```

### 🛡️ High Availability & Smart Retries

- **Automatic failover**: If a backend goes down, traffic is automatically routed to healthy backends
- **Smart retries**: Auto-retry on 429 rate limits, 5xx server errors, and timeouts with exponential backoff
- **Zero code changes**: Simply point your API Base URL to AI Gateway — no client modifications needed

---

## ✨ More Features

- **One-click platform setup**: 15+ built-in AI platform presets (including Google Gemini)
- **Remote model fetching**: Automatically fetch available models from platform APIs when configuring backends — select from dropdown, no manual input
- **Smart capability detection**: Model capabilities (chat/code/vision/function calling) auto-filled from presets, with manual override support
- **reasoning_content support**: Compatible with chain-of-thought content from NVIDIA NIM and similar platforms, auto-forwarded in responses
- **Dark/Light mode**: Supports light, dark, and follow-system themes
- **Bilingual UI**: Full i18n support — switch between Chinese and English with one click
- **Configurable port**: Admin port can be changed from the UI; default is 1994
- **Cross-platform desktop app**: Native support for macOS / Windows / Linux (built on Tauri)
- **Standalone server**: Single binary, zero dependencies — ideal for server deployment

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────┐
│                AI Gateway                     │
│                                              │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ OpenAI  │  │Anthropic│  │  Admin Web   │ │
│  │Endpoint │  │Endpoint │  │  Dashboard   │ │
│  └────┬────┘  └────┬────┘  └──────────────┘ │
│       │            │                         │
│  ┌────▼────────────▼────┐                    │
│  │  Routing & Load       │                    │
│  │  Balancing Engine     │                    │
│  └────┬───┬───┬────┬────┘                    │
│       │   │   │    │                         │
│  ┌────▼┐ ┌▼──┐┌▼───┐┌▼────┐                │
│  │Deep │ │Qwen││GLM ││GPT │  ← Multiple     │
│  │Seek │ │    ││    ││-4o │    backends      │
│  └─────┘ └───┘└────┘└─────┘                │
└──────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### Option 1: Standalone Server Mode

```bash
# Clone the repository
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Build and run
cargo run

# Open the admin dashboard
open http://localhost:1994
```

### Option 2: Tauri Desktop App Mode

```bash
# Install Tauri CLI
cargo install tauri-cli

# Development mode
cargo tauri dev

# Build desktop app
cargo tauri build
```

### Option 3: Frontend Development Mode

```bash
# Terminal 1: Start the backend
cargo run

# Terminal 2: Start the frontend dev server
cd frontend
npm install
npm run dev
```

---

## 🎯 Typical Use Cases

### Use Case 1: Multi-Key Free Tier Rate Limit Bypass

> Platforms like SiliconFlow and Groq offer free credits but impose severe rate limits (3–30 RPM). Register multiple accounts to get multiple keys, then use AI Gateway's load balancing to distribute requests — multiplying your total throughput by N.

1. In "Platform Management", add multiple configurations for the same platform (e.g., 3 SiliconFlow entries), each with a different API Key
2. Create a virtual model and select the "Round Robin" strategy
3. When adding backends, select each platform and pick models from the dropdown (auto-fetched) — done in one click
4. Start the endpoint — 3 keys are used in rotation, rate limit threshold increased 3×.

### Use Case 2: Cost Optimization — Cheap First, Expensive Fallback

> DeepSeek is extremely affordable but occasionally unstable; OpenAI is reliable but costly. Use the "Priority" strategy to route traffic to DeepSeek first, with automatic failover to OpenAI on errors.

### Use Case 3: Unified Entry for Multi-Platform

> Your application needs both OpenAI's GPT-4o and Anthropic's Claude. AI Gateway supports both protocols simultaneously — one gateway covers everything.

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

> 💡 The admin port can also be changed directly from the "Settings" page in the desktop app — no need to manually edit the config file.

---

## 📖 Usage Guide

### 1️⃣ Add a Platform

Go to "Platform Management" → Click "Add Platform" → Select a preset or create custom → Enter your API Key → Save

> 💡 You can add the same platform multiple times (each with a different API Key) for multi-key load balancing.

### 2️⃣ Create a Virtual Model

Go to "Virtual Models" → Click "New Virtual Model" → Set model name → Select load balancing strategy → Add backend models

> 💡 When adding backend models, the system automatically fetches available models from the selected platform. Just pick from the dropdown — no need to type model IDs manually. Model capabilities are auto-filled from presets and can be manually adjusted.

### 3️⃣ Call the API

```bash
# OpenAI-compatible format (supports all OpenAI SDKs)
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic-compatible format (supports all Anthropic SDKs)
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# List models
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
```

**Python Examples:**

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
│   ├── db/               # Database layer (r2d2 connection pool)
│   ├── proxy/            # Proxy handlers
│   ├── lb/               # Load balancing engine
│   ├── protocol/         # OpenAI/Anthropic protocol adapters
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
