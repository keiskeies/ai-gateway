# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AI Gateway is a cross-platform AI API aggregation and load-balancing gateway. It exposes OpenAI-compatible (`/v1/chat/completions`) and Anthropic-compatible (`/v1/messages`) endpoints that route requests to multiple AI backends with load balancing, automatic failover, and retry logic. Runs as both a standalone server and a Tauri 2.0 desktop app (macOS/Windows/Linux).

## Build & Run Commands

```bash
# Standalone server (port 1994, serves API + static frontend)
cargo run

# Frontend dev server (port 5173, proxies /api /v1 /health to localhost:1994)
cd frontend && npm install && npm run dev

# Build frontend (outputs to ../static/, NOT frontend/dist/)
cd frontend && npm run build

# Tauri desktop app (dev mode, frontend on port 1420)
cargo install tauri-cli
cargo tauri dev

# Release build (standalone binary)
cargo build --release

# Release build (Tauri desktop app)
cargo tauri build
```

No tests, linter, or formatter are configured. CI (`release.yml`) only builds and creates GitHub releases.

## Architecture

### Two Rust Crates

- **Root crate** (`src/`): Both a library (`ai_gateway`) and a standalone binary (`ai-gateway`). The binary sets up actix-web, DB pool, HTTP client, `RouteCache`, `StatsWriter`, and `ProxyState`, then registers all routes.
- **`src-tauri/`**: Tauri desktop wrapper that depends on the root crate via `path = ".."`. Starts the same actix-web server in a background thread (using `BACKEND_PORT`/`BACKEND_READY` atomics for synchronization), then navigates the Tauri webview to `http://127.0.0.1:{port}`. The server setup code is duplicated between `src/main.rs` and `src-tauri/src/lib.rs` (not shared).

### Request Flow (Zero DB on Hot Path)

1. Client calls `/v1/chat/completions` or `/v1/messages` with `model` = virtual model name
2. Auth: `validate_auth_cached()` checks API key against `RouteCache` (pure `DashMap` lookup, zero DB)
3. Request parsed to `UnifiedRequest` via `protocol/openai.rs` or `protocol/anthropic.rs`
4. `cache.get_resolved_route()` retrieves the full route with all backends+platforms pre-joined from `RouteCache` (zero DB)
5. Retry loop: `BackendSelector::select()` picks a backend, `build_forward_request()` converts to target platform protocol, sends via `http_client`
6. Connection count tracked via `inc_connection`/`dec_connection` on the `DashMap`-backed selector
7. On success: response converted from platform protocol back to client's requested protocol
8. On failure (429/5xx/timeout): retry with exponential backoff (`backoff_ms * 2^attempt`) if error matches retry policy
9. Stats recorded via `stats_writer.record()` (async, non-blocking — batched and flushed every 500ms)
10. Streaming responses: passed through as-is (SSE passthrough, no protocol conversion)

### Module Map

| Module | Purpose |
|--------|---------|
| `src/api/` | Actix-web REST handlers for `/api/*` CRUD endpoints + `settings.rs` for runtime config |
| `src/cache.rs` | `RouteCache` (in-memory proxy→route→backend resolution) + `StatsWriter` (async batch stats) |
| `src/db/` | SQLite data access layer (rusqlite + r2d2 pool), schema migrations V1–V5 |
| `src/proxy/` | Proxy request handler, `ProxyState` shared state |
| `src/lb/` | Load balancing: `BackendSelector` with 5 strategies (DashMap + AtomicUsize counters) |
| `src/protocol/` | OpenAI/Anthropic protocol adapters + `UnifiedRequest`/`UnifiedResponse` types |
| `src/models/` | Data models (Platform, Model, Proxy, Route, Backend, ApiKey, Stats) + 15 platform presets |
| `src/config.rs` | `AppConfig` with `load_or_default()`, path resolution, system proxy detection (env vars + Windows registry) |
| `src/error.rs` | `AppError` enum with `ResponseError` impl, `AppResult<T>` alias |

### Key Architectural Patterns

**RouteCache (write-through, full rebuild on mutation):** All admin CRUD handlers call `cache.refresh()` after mutating the DB. The cache stores `proxy_name → ResolvedRoute` in `DashMap` for lock-free concurrent reads. `ResolvedRoute` pre-joins all backends with their full `Platform` info so the proxy handler never touches the DB. API keys are cached separately via `refresh_api_keys()`. `touch_api_key()` updates `last_used` in DB via `tokio::spawn` to avoid blocking.

**StatsWriter (async batch):** Uses `tokio::sync::mpsc::unbounded_channel`. `record()` is non-blocking; if the channel is full, stats are silently dropped rather than blocking proxy requests. Background task flushes to DB every 500ms.

**SharedAppConfig:** `Arc<RwLock<AppConfig>>` (parking_lot) allows runtime config updates via `PUT /api/settings`. Write lock is released before file I/O to avoid holding the lock during disk writes.

**Protocol field passthrough:** Both `OpenAIChatRequest` and `AnthropicRequest` use `#[serde(flatten)] extra: serde_json::Map` to capture unknown fields. When converting to the target protocol, `extra` fields are merged without overwriting known fields — enabling end-to-end passthrough of vendor-specific parameters (e.g., `reasoning_effort`, `top_k`).

**API key encryption:** Platform API keys are encrypted at rest using `aes-gcm` with the `encrypt_key` from `config.toml`.

**System proxy detection:** `SystemProxy::detect()` checks `HTTP_PROXY`/`HTTPS_PROXY` env vars first. On Windows, falls back to reading the registry (`HKCU\...\Internet Settings`). Applied to `reqwest::ClientBuilder` at startup.

### Data Model Relationships

- **Platform** → has many **Models** (model_id is a string like "gpt-4o", not a UUID)
- **Proxy** (virtual model) → has one **Route** (1:1 via proxy_id UNIQUE)
- **Route** → has many **Backends** (each backend references a platform_id + model_id string)
- **ApiKey** → optionally bound to a proxy_id (null = global access)
- Backend.model_id stores the actual model ID string (e.g., "gpt-4o"), NOT a foreign key to the models table (changed in V5 migration)
- V5 also added `capabilities` column to backends

### Load Balancing Strategies

Five strategies in `BackendSelector`. `LatencyBased` currently falls back to round-robin (not yet latency-aware). Strategies use `DashMap<String, AtomicUsize>` for connection tracking with keys formatted as `{route_id}:{backend_id}`.

### Frontend

- React 19 + TypeScript + Mantine 7 + Vite 8
- Icons: `@tabler/icons-react`
- Routing: `react-router-dom` v7 with `HashRouter` (required for Tauri webview compatibility)
- Styling: Mantine + Tailwind CSS 4
- Pages: Dashboard, Platforms, Proxies (virtual models), ApiKeys, Settings
- `src/api.ts`: Axios-based API client. Detects Tauri mode via `window.__TAURI_INTERNALS__`. In Tauri mode uses same-origin relative paths; in browser dev uses Vite proxy
- `src/i18n.ts`: Chinese/English translations
- `src/presets.ts`: 15+ platform presets with model lists
- `src/ThemeContext.tsx`: Theme (dark/light/system) and locale (zh/en) context provider. Default: dark theme, Chinese locale
- Builds to `../static/` (configured in `vite.config.ts`)

## Key Configuration

- `config.toml` at app directory root. Auto-generated on first run if missing.
- `AppConfig::load_or_default()` searches upward from exe dir for `config.toml` (up to 3 levels, handles macOS .app bundles, Windows install paths, Tauri resource dirs)
- Tauri mode: `set_app_dir()` is called with the resolved resource path before any config loading
- Default port: 1994. Configurable in `config.toml` and Settings page.
- `SharedAppConfig` (`Arc<RwLock<AppConfig>>`) enables runtime updates without restart

## Gotchas

- Frontend builds to `../static/` not `frontend/dist/`. The `emptyOutDir: true` in vite.config.ts means `static/` gets wiped on each frontend build. Don't put anything in `static/` that isn't generated by the build.
- On Windows, config resolution may find `C:\Users\<user>\AppData\Local\AI Gateway\config.toml` before the project-local one when running as a Tauri app.
- `config.toml` `encrypt_key` and `admin_token` are security-sensitive. Never commit real values.
- DB uses WAL mode and foreign keys. Migrations are version-tracked in `_schema_version` table.
- If no API keys exist in the database, all proxy requests are allowed without authentication.
- Streamed responses are passed through as-is (SSE passthrough) — no protocol conversion happens for streams.
- `reasoning_content` is explicitly handled for NVIDIA reasoning models — falls back from `content` to `reasoning_content` in `extract_message_content()`.
- The server setup code is duplicated between `src/main.rs` and `src-tauri/src/lib.rs`. Changes to one must be mirrored in the other.
- Dev profile has `opt-level = 2` for all dependencies — faster runtime but slower initial compile.
- `beforeBuildCommand` in tauri.conf.json is empty — frontend must be pre-built before `cargo tauri build`.
