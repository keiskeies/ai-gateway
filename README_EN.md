<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>One Gateway to Aggregate All AI Platforms</strong><br/>
  Multi-Key Load Balancing · Auto Failover · OpenAI & Anthropic Dual Protocol · Zero Code Changes
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
  <a href="#-quick-start"><strong>Get Started in 30 Seconds →</strong></a>
</p>

---

> 🌟 **If this project helps you, please give it a Star! Your support drives continuous improvement.**

---

## 😩 Sound Familiar?

- 🔴 **429 rate limits** hitting you mid-conversation
- 🔴 Free tier **3 RPM is nowhere near enough**
- 🔴 DeepSeek is cheap but flaky, OpenAI is stable but pricey — **no easy way to switch**
- 🔴 Multiple AI platforms with different API formats = **messy if-else in your code**
- 🔴 Want to use both Claude and GPT, but **managing multiple SDKs and base URLs is a pain**

**AI Gateway was built to solve all of these.**

---

## 📸 Screenshots

<p align="center">
  <img src="doc/ai-gateway1.jpg" width="80%" alt="AI Gateway Main Interface" />
</p>
<p align="center"><em>Main Interface — Virtual Model Management</em></p>

<details>
<summary>📱 Click to see more screenshots</summary>

<p align="center">
  <img src="doc/ai-gateway2.jpg" width="80%" alt="AI Gateway Platform Management" />
</p>
<p align="center"><em>Platform Management — Add & Configure AI Platforms</em></p>

<p align="center">
  <img src="doc/a-gateway-1.jpg" width="80%" alt="AI Gateway Virtual Model Configuration" />
</p>
<p align="center"><em>Virtual Models — Backend Configuration & Load Balancing Strategies</em></p>

<p align="center">
  <img src="doc/ai-gateway21.jpg" width="80%" alt="AI Gateway Statistics Overview" />
</p>
<p align="center"><em>Statistics Overview — Token Usage & Request Stats</em></p>

<p align="center">
  <img src="doc/ai-gateway3.jpg" width="80%" alt="AI Gateway Settings Page" />
</p>
<p align="center"><em>Settings Page — Port Configuration & Theme Switching</em></p>

</details>

---

## ⚡ Core Capabilities

### 🎯 Multi-Key Load Balancing — Rate Limits? What Rate Limits?

Distribute requests across multiple API Keys — **N keys = N× throughput**:

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

> 💡 **Free tier hack**: Multiple free accounts on SiliconFlow, Groq, etc. × load balancing = unlimited free AI

### 🔀 5 Load Balancing Strategies

| Strategy | TL;DR | Best For |
|----------|--------|----------|
| **Round Robin** | Take turns, fair and square | Multi-key rate limit bypass |
| **Weighted Random** | Higher weight = more work | Uneven backend performance |
| **Least Connections** | Give it to whoever's idle | Streaming output scenarios |
| **Priority** | Cheap first, expensive as fallback | Save money! DeepSeek → GPT fallback |
| **Latency Based** | Pick the fastest | Latency-sensitive online services |

### 🌐 OpenAI & Anthropic Dual Protocol — One Entry Point for Everything

```
OpenAI SDK ──→ /v1/chat/completions ──┐
                                       ├──→ AI Gateway ──→ DeepSeek / Qwen / GPT / Claude ...
Anthropic SDK ──→ /v1/messages ───────┘
```

**Zero code changes** — just swap `base_url` to `http://localhost:1994/v1` and you're done.

### 🛡️ High Availability — No Single Point of Failure

- Backend down? **Auto-failover** to healthy backends
- Rate limited? 429 / 5xx / timeout **auto-retry** with exponential backoff
- Fully transparent: your app never notices backend switches

---

## 🆚 Comparison with Alternatives

| | AI Gateway | Nginx Reverse Proxy | One API | LiteLLM |
|---|---|---|---|---|
| **Ready to use** | ✅ Desktop App + Server | ❌ Manual config | ✅ | ✅ |
| **Visual management** | ✅ Web UI | ❌ | ✅ | ❌ |
| **Load balancing** | ✅ 5 strategies | ✅ Limited | ⚠️ Basic | ✅ |
| **OpenAI protocol** | ✅ | ✅ | ✅ | ✅ |
| **Anthropic protocol** | ✅ Native support | ❌ Extra config | ✅ Conversion | ✅ Conversion |
| **Auto failover** | ✅ | ⚠️ Manual config | ⚠️ | ⚠️ |
| **Remote model fetching** | ✅ Auto-fetch | ❌ | ❌ | ❌ |
| **Local deployment** | ✅ Single binary, zero deps | ✅ | ✅ Docker | ✅ pip |
| **Desktop app** | ✅ macOS/Win/Linux | ❌ | ❌ | ❌ |
| **Language** | Rust (high perf, low memory) | C | Go | Python |

---

## ✨ More Features

- 🏪 **15+ platform presets**: OpenAI / Anthropic / DeepSeek / Qwen / Gemini / GLM / Moonshot / Doubao / Ollama / NVIDIA NIM / Azure / SiliconFlow / Groq / Yi / Baichuan ··· one-click add
- 🔍 **Remote model fetching**: Auto-fetch available models from platform APIs — pick from dropdown, no manual input
- 🧠 **Smart capability detection**: Chat/code/vision/function-calling capabilities auto-filled from presets
- 🔗 **reasoning_content support**: Compatible with chain-of-thought output from NVIDIA NIM etc.
- 🌙 **Light / Dark / System** theme modes
- 🌍 **Chinese & English** bilingual UI
- ⚙️ Configurable port from the UI, default 1994

---

## 🚀 Quick Start

### Option 1: Download Desktop App (Recommended)

Head to [Releases](https://github.com/keiskeies/ai-gateway/releases) and download the installer for your platform.

### Option 2: Build from Source

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Standalone server mode
cargo run
# Visit http://localhost:1994

# Or Tauri desktop app mode
cargo install tauri-cli
cargo tauri dev
```

### Three Steps to Get Going

```
1️⃣ Add Platform → Pick preset + Enter API Key → Save
2️⃣ Create Virtual Model → Pick strategy + Add backends → Start
3️⃣ Change base_url → Point to http://localhost:1994/v1 → Done
```

---

## 📖 API Usage

Change one line of code to integrate — **nothing else needed**:

```python
# OpenAI SDK — just change base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← Change only this line
    api_key="your-token"
)
response = client.chat.completions.create(
    model="your-virtual-model",  # ← Your virtual model name
    messages=[{"role": "user", "content": "hello"}]
)

# Anthropic SDK — same, just change base_url
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",  # ← Change only this line
    api_key="your-token"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 curl Examples</summary>

```bash
# OpenAI-compatible format
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic-compatible format
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# List models
curl http://localhost:1994/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
```

</details>

---

## 🎯 Who Uses This? Typical Scenarios

| Scenario | How | Benefit |
|----------|-----|---------|
| **Free-tier maximizer** | Round-robin across free keys | Free quota × N |
| **Cost saver** | DeepSeek first + GPT fallback | 80%+ cost reduction |
| **Reliability seeker** | Multi-backend + auto-failover | 99.9% availability |
| **Multi-platform user** | Unified OpenAI + Claude entry | One line to switch |
| **Team manager** | Shared gateway + API key management | Secure & controllable |

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │ Endpoint │  │ Endpoint  │  │  Dashboard   │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │   Routing & Load Balance │                     │
│  │        Engine            │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N backends  │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
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

> 💡 The admin port can also be changed from the "Settings" page in the desktop app — no manual config editing needed.

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust · Actix-Web · SQLite (r2d2) · Reqwest |
| Frontend | React · TypeScript · Ant Design · Vite |
| Desktop | Tauri 2.0 |
| Database | SQLite (rusqlite + r2d2 connection pool) |

---

## 🗺️ Roadmap

- [ ] 📊 Richer statistics dashboard (daily/weekly/monthly usage trends)
- [ ] 🔑 Fine-grained API key permissions (per-model / per-quota rate limiting)
- [ ] 🌐 Multi-modal gateway (translation / TTS / embedding unified endpoints)
- [ ] 🔔 Backend health checks & anomaly alerting
- [ ] 🐳 Docker one-click deployment
- [ ] ☁️ Cloud config sync

> Have a feature request? [Open an Issue](https://github.com/keiskeies/ai-gateway/issues)!

---

## 🤝 Contributing

Contributions of all kinds are welcome!

1. Fork this repo
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## 📂 Project Structure

```
ai-gateway/
├── src/                  # Rust backend
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
│       └── pages/        # Page components
├── src-tauri/            # Tauri desktop app
├── doc/                  # Screenshots & docs
├── config.toml           # Configuration file
└── data/                 # SQLite database
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
  Found this useful? Give it a <a href="https://github.com/keiskeies/ai-gateway">⭐ Star</a>!
</p>
