# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Read AGENTS.md too

`AGENTS.md` is maintained alongside the code and holds the exhaustive per-feature gotcha log (client adapters, check-in/credits, wall layout, Mantine contrast traps, migration ordering, Windows MSVC build). This file covers the big picture; `AGENTS.md` covers what breaks. Read both before changing a subsystem. Where the two disagree, `AGENTS.md` and the code are right.

## Project Overview

AI Gateway is a cross-platform AI API aggregation and load-balancing gateway. OpenAI-compatible (`/v1/chat/completions`, `/v1/completions`) and Anthropic-compatible (`/v1/messages`) endpoints route to multiple backends with load balancing, failover, retries, cost/credit accounting, and a web console. Ships as a standalone binary and a Tauri 2 desktop app (macOS/Windows/Linux).

## Build & Run Commands

```bash
cargo run                          # standalone server, port 1994, serves API + static/
cargo build                        # target/debug/ai-gateway  (required before E2E)
cd frontend && npm install && npm run dev    # Vite on 5173, proxies /api /v1 /health → 1994
cd frontend && npm run build        # tsc -b && vite build → outputs to ../static/ (NOT frontend/dist/)
cargo tauri dev                    # desktop app (frontend on 1420); cargo install tauri-cli first
cargo tauri build                  # installers in src-tauri/target/release/bundle/, NOT target/
```

**Frontend must be built before `cargo run` or `cargo tauri build`** — the binary serves `static/`, and `tauri.conf.json`'s `beforeBuildCommand` is empty.

### Tests

```bash
cargo test                         # ~360 lib tests, in #[cfg(test)] modules next to the code
cargo test <name>                  # single test
cargo test --test <name>           # integration (none today)
cd src-tauri && cargo test         # MUST run from src-tauri/ — see below
```

There is no clippy, rustfmt, eslint, or prettier configured. CI (`release.yml`) runs on `v*` tags and only builds/packages + creates the release — **it does not run tests**.

`src-tauri/` is its own cargo workspace with its own `Cargo.lock`. From the repo root, `cargo check -p ai-gateway-tauri` fails with "package ID specification did not match any packages". It has ~2 unit tests and is not touched by E2E, so it drifts silently — after changing server setup in the root crate, run tests from inside `src-tauri/`. On Windows both crates need the MSVC environment plus a writable `TEMP` and an out-of-repo `CARGO_TARGET_DIR` (see AGENTS.md; the wrapper is gitignored as `/build.bat`).

### Browser E2E (pytest + Playwright)

```bash
cargo build && npm --prefix frontend run build   # fixtures do NOT build these; they fail with which one is missing
python -m pytest e2e -q                          # 61 specs, ~4 min
python -m pytest e2e/test_wall.py -q             # one file
UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py   # regenerate pixel baselines
```

`e2e/conftest.py` starts isolated instances on dynamic ports with their own `config.toml`/`data/`, and refuses to bind 1994 (installed prod) or 1995 (manual preview). `EXE` picks `ai-gateway.exe` only on `os.name == "nt"`, so the suite launches on macOS/Linux without edits. Pixel baselines live in `.dsh-tmp/visual/` (gitignored): a fresh checkout generates them on the first `test_visual.py` run and skips the comparison that pass. Non-obvious harness rules (fake clicks vs React, `[role=dialog]`, `?v=N` hash navigations, the ~500ms StatsWriter lag, axe's bracket-stripping in accessible names, `matchMedia`-derived breakpoints) are documented in AGENTS.md.

## Architecture

### Two Rust crates

- **Root crate** (`src/`): both a library (`ai_gateway`) and a binary (`ai-gateway`). `src/main.rs` wires the DB pool, HTTP client, `RouteCache`, `StatsWriter`, `ProxyState`, and routes.
- **`src-tauri/`**: Tauri wrapper, depends on the root crate via `path = ".."`. Starts the same actix-web server on a background thread (`BACKEND_PORT`/`BACKEND_READY` atomics), then points the webview at `http://127.0.0.1:{port}`. **The server setup is duplicated between `src/main.rs` and `src-tauri/src/lib.rs` — changes to one must be mirrored.**

### Request flow (zero DB on the hot path)

1. Client calls `/v1/chat/completions` or `/v1/messages` with `model` = virtual model name.
2. `validate_auth_cached()` checks the gateway API key against `RouteCache` (DashMap lookup, no DB).
3. Body parsed into `UnifiedRequest` by `protocol/openai.rs` or `protocol/anthropic.rs`.
4. `cache.get_resolved_route()` returns the route with every backend already joined to its `Platform`.
5. Retry loop: `BackendSelector::select()` → `PlatformAdapter` builds the forward request → `http_client` sends.
6. Same-protocol non-stream responses are passed through raw after rewriting `model`; cross-protocol responses are rebuilt from the unified format. Streams are same-protocol → passthrough, cross-protocol → `proxy/stream_convert.rs` event by event.
7. Failure matching the retry policy (429 / 5xx / 408) retries with `backoff_ms * 2^attempt`, re-entering the load balancer.
8. `stats_writer.record()` is non-blocking (batched, flushed every 500ms; dropped rather than blocking the request).

### Module map

| Module | Purpose |
|--------|---------|
| `src/api/` | Actix handlers for `/api/*`; `mod.rs` is the route table; `settings.rs` holds `SharedAppConfig` |
| `src/cache.rs` | `RouteCache` (proxy→route→backend, write-through full rebuild) + `StatsWriter` |
| `src/catalog.rs` | `catalog_key = SHA256(type_key \| normalize_base_url)` — shared model/pricing catalog |
| `src/checkin/` | Desktop-only daily check-in (ModelScope) + `spawn_checkin_runner` 60s scheduler |
| `src/clients/` | One adapter per local coding client (Claude Code, Codex, OpenCode, OpenClaw, Pi, Hermes) + `service.rs` |
| `src/db/` | rusqlite + r2d2; `schema.rs` holds all migrations |
| `src/lb/` | `BackendSelector`, five strategies over `DashMap<String, AtomicUsize>` keyed `{route_id}:{backend_id}` |
| `src/models/` | Data types + platform presets + `adapter.rs` (`PlatformAdapter`) |
| `src/protocol/` | OpenAI/Anthropic adapters, `UnifiedRequest`/`UnifiedResponse`, `reasoning.rs`, `content.rs`, `nonce.rs` |
| `src/proxy/` | `handler.rs` (the main chain), `stream_convert.rs`, `reasoning_cache.rs`, `upstream/` |
| `src/proxy/upstream/` | Private-protocol upstreams (Trae, WorkBuddy). Adapters only build the request — retries, credits and stats stay in `handler.rs`, so adding an upstream never touches the main chain |
| `src/api/local.rs` | `require_local_console` guard (loopback peer **and** loopback `Host` **and** no `X-Forwarded-*`) |

### Patterns worth knowing before editing

**RouteCache is rebuild-on-write.** Every admin mutation handler calls `cache.refresh()` (or `refresh_api_keys()`); reads are lock-free `DashMap` lookups against a fully pre-joined `ResolvedRoute`. Never add a per-request DB read to the proxy path.

**StatsWriter is fire-and-forget.** `mpsc::unbounded_channel`; a full channel drops stats rather than blocking a proxy response. Anything reading fresh stats must tolerate ~500ms of lag.

**New upstream = new `PlatformAdapter`, not a new handler.** `upstream::ForwardPlan` carries url/body/headers/`force_stream`/`always_sse`; returning `None` from `build` falls through to the generic OpenAI/Anthropic path.

**Protocol passthrough is deliberate.** Both request types use `#[serde(flatten)] extra: serde_json::Map`; when converting, `extra` is merged *without* overwriting known fields, so vendor params (`reasoning_effort`, `top_k`) survive end to end.

**Reasoning is client-driven.** Inbound OpenAI/OpenRouter/vendor shapes normalize into `protocol/reasoning.rs`; outbound format is chosen by `backend.reasoning_style` (`Auto` → Anthropic upstream gets `thinking`, everything else `reasoning_effort`), and `param_overrides` merges last and wins. Anthropic thinking signatures replay unchanged; cross-protocol tool loops use `proxy/reasoning_cache.rs` (30 min, keyed by `tool_use.id`, fail-open by disabling thinking). Thinking content is never logged.

**Auth is split in two.** `/api/*` is behind session middleware (Argon2id `users`, `sessions` rows + HttpOnly `aigw_session` cookie, 7d); only `/api/auth/status|login|setup` are public and `setup` is loopback-only. `/v1/*` uses gateway API keys instead (`/v1/models` and `/health` public). `/api/clients/*` additionally requires the local-console guard.

**Data model.** Platform → many Models; Proxy (virtual model) → one Route (1:1 via `proxy_id UNIQUE`) → many Backends. `Backend.model_id` is a plain string, not an FK. Models belong to a *shared catalog*, not a platform: reach models through `p.catalog_key`, never `b.platform_id` (AGENTS.md, "shared catalog").

**Cost is a frozen snapshot.** `cost = (fresh*input + hit*cache_read + write*cache_write + out*output) / 1e6`, with null cache prices falling back to `input_price`. The pricing tier is captured once at request start (`pricing_at`, local timezone, windows may wrap midnight) and threaded into `handle_stream`, so a price edit never rewrites an in-flight or historical cost.

### Load balancing reality

`RoundRobin`, `WeightedRandom`, `LeastConnections`, `Priority`, `LatencyBased` exist in `models/route.rs`. `LatencyBased` is round-robin (latency is not sampled), `Priority` never degrades to the next backend on failure, there is no health-check-driven ejection, and connection counts release at response **headers**, not at end of stream. Do not "fix" these silently — they are documented as known boundaries in README.md.

### Frontend

React 19 + TS + Mantine 7 + Tailwind 4 + Vite 8, `@tanstack/react-query`, `@tabler/icons-react`, `HashRouter` (required for the Tauri webview). Pages in `frontend/src/pages` (流量 / 监控大屏 `/wall` / 虚拟模型 / 平台 / API 密钥 / Playground / 客户端接入 / 设置 / 登录), API client in `src/api.ts` (detects `window.__TAURI_INTERNALS__`, else Vite proxy), zh/en strings in `src/i18n.ts`, theme+locale provider in `src/ThemeContext.tsx` (default dark/zh).

Front-end rules that tooling enforces: every user-facing string goes through `t(locale, key)`; form fields use `components/Field.tsx` (hand-written sibling `<label>` is invisible to screen readers and axe flags it); icon-only controls carry their own `aria-label` (a Mantine `Tooltip` only adds `aria-describedby`); type floor is 11px; a failed fetch renders "did not load" / `—`, never `0`.

## Configuration

`config.toml` at the app directory root, auto-generated on first run (existing files are never overwritten); Settings writes back to it. `AppConfig::load_or_default()` searches **upward from the exe directory** (up to 3 levels — handles macOS `.app/Contents/Resources/_up_/`, Windows install paths, Tauri resources). Default port 1994 (`server.admin_port`); changing port or listen address needs a restart, everything else is live via `SharedAppConfig` (`Arc<parking_lot::RwLock<AppConfig>>`) and `PUT /api/settings`.

## Conventions & Gotchas

- **Chinese comments throughout the Rust backend** — keep comments in Chinese when editing existing files. Commit messages follow Conventional Commits with a Chinese subject (`feat(traffic): 请求详情各数据项加复制按钮`).
- **Never touch `static/` by hand** — it is gitignored and `emptyOutDir: true` wipes it on every frontend build.
- Version numbers live in several places and must move together (CI fails on mismatch): `Cargo.toml` (**the source of truth** — `vite.config.ts` parses `version` out of it and injects `__APP_VERSION__`, so the UI header needs no edit), `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `frontend/package.json`, and the release-notes header in `.github/workflows/release.yml`. Then `cargo generate-lockfile` in **both** workspaces (the two `Cargo.lock` files are independent), rebuild the frontend, tag `vX.Y.Z`, and `gh release create`.
- **Migrations** (`db/schema.rs`, currently V1–V39) are version-tracked in `_schema_version`; each is an `if current_version < N` block. Never reuse a number: two `if current_version < N` blocks with the same N means the second one is silently skipped on every database that already ran the first, and git merges the two branches without a single conflict because the blocks are textually separate. New columns go through `add_column_if_missing` because tests rewind `_schema_version` and re-run the block, and statement order inside a migration can be load-bearing (see V25's backfill → dedupe → unique index).
- DB is SQLite WAL with `foreign_keys=ON`, `busy_timeout=5000`, `synchronous=NORMAL`, pool `max_size=8`, 5s connection timeout.
- **If no API keys exist, requests are still required to carry `Authorization` or `x-api-key`** — the value is just not checked.
- `reasoning_content` (NVIDIA NIM and similar) is handled in `extract_message_content()`; Anthropic extended thinking requires `budget_tokens >= 1024` and `max_tokens > budget_tokens` and rejects `temperature`/`top_p`/`top_k` — `anthropic::to_request` clamps the budget downward and strips those params.
- `encrypt_key` and `admin_token` in `config.toml` are security-sensitive; never commit real values. No `data/` or `*.db` files in git.
- Windows config resolution can pick `C:\Users\<user>\AppData\Local\AI Gateway\config.toml` over the project-local one when running as a Tauri app; a loose `src-tauri/target/release/ai-gateway-tauri.exe` resolves its app dir to `target/release/_up_/` and therefore reads the *project* `config.toml`.
- Dev profile has `opt-level = 2` for all dependencies — faster runtime, slower first compile.
