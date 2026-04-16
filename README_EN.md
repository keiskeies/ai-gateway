<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.77+-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-5.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Cross-Platform AI API Aggregation & Intelligent Traffic Load Balancer</strong><br/>
  Unified access to OpenAI · Anthropic · DeepSeek · Qwen · Moonshot · ZhipuAI · Doubao · Ollama · NVIDIA NIM · Azure · and more...
</p>

---

## ✨ Key Highlights

### 🔀 AI API Traffic Load Balancing

The core capability of AI Gateway — **aggregate multiple AI backends into a single entry point, intelligently distribute request traffic**:

| Strategy | Description | Use Case |
|----------|-------------|----------|
| **Round Robin** | Distribute requests sequentially across backends | Equal-capacity backends, even load distribution |
| **Weighted Random** | Random distribution weighted by backend capacity | Heterogeneous backends, quota-based allocation |
| **Least Connections** | Prefer the backend with fewest active connections | Long-lived/streaming scenarios, prevent overload |
| **Priority** | Active-standby mode, auto-failover to lower priority | Cost optimization: cheap model first, expensive as fallback |
| **Latency Based** | Track real-time response latency, prefer fastest backend | Latency-sensitive online services |

**Typical Flow:**

```
Client Request → AI Gateway → Route Match → Load Balance → Backend Select → Forward → Stream Back
                    ↓
      ┌─────────────┼─────────────┐
      ↓             ↓             ↓
   DeepSeek       Qwen        OpenAI    ← Same virtual model, multiple backends
   (weight 3)   (weight 2)   (weight 1) ← Weighted random strategy
```

- **High Availability**: Automatic failover when a backend goes down
- **Cost Optimization**: Prioritize cost-effective models, expensive ones as fallback
- **Zero Code Change**: Simply point your API Base URL to Gateway — no client modification needed
- **Multi-Protocol**: Supports both OpenAI and Anthropic API formats simultaneously

### 🌐 More Features

- **One-Click Platform Setup**: 12+ built-in AI platform presets
- **Model Presets**: Auto-fill popular model IDs, no manual lookup needed
- **Dark/Light/System Theme**: Three modes — light, dark, follow system
- **i18n**: Full Chinese and English support, one-click switch
- **Cross-Platform Desktop**: Native support for macOS / Windows / Linux (Tauri)
- **Standalone Server**: Single binary, zero dependencies, perfect for server deployment

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────┐
│                  AI Gateway                   │
│                                              │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ OpenAI  │  │Anthropic│  │  Admin Web   │ │
│  │Endpoint │  │Endpoint │  │    UI        │ │
│  └────┬────┘  └────┬────┘  └──────────────┘ │
│       │            │                         │
│  ┌────▼────────────▼────┐                    │
│  │  Routing & LB Engine │                    │
│  └────┬───┬───┬────┬────┘                    │
│       │   │   │    │                         │
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
# Clone the repo
git clone https://github.com/your-username/ai-gateway.git
cd ai-gateway

# Build and run
cargo run

# Open admin UI
open http://localhost:1994
```

### Option 2: Tauri Desktop App

```bash
# Install Tauri CLI
cargo install tauri-cli

# Development mode
cargo tauri dev

# Build desktop app
cargo tauri build
```

### Option 3: Frontend Development

```bash
# Terminal 1: Start backend
cargo run

# Terminal 2: Start frontend dev server
cd frontend
npm install
npm run dev
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

---

## 📖 Usage Guide

### 1️⃣ Add Platform

Go to "Platforms" → Click "Add Platform" → Select preset or custom → Enter API Key → Save

### 2️⃣ Add Model

Go to "Models" → Click "Add Model" → Select platform → Choose from presets or enter model ID → Save

### 3️⃣ Create Proxy

Go to "Proxies" → Click "New Proxy" → Set name and port → Select protocols (OpenAI/Anthropic)

### 4️⃣ Configure Routes

In proxy details → Click "Add Route" → Set virtual model name → Select LB strategy → Add backend models

### 5️⃣ Call the API

```bash
# OpenAI compatible format
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic compatible format
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# List models
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
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
│   ├── api/              # REST API handlers
│   ├── db/               # Database layer (r2d2 pool)
│   ├── proxy/            # Proxy handler
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
