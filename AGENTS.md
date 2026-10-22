# AGENTS.md

## Project overview

AI Gateway: a Rust + React + Tauri desktop/server app that load-balances requests across multiple AI platforms (OpenAI, Anthropic, DeepSeek, etc.). Exposes OpenAI-compatible `/v1/chat/completions` and Anthropic-compatible `/v1/messages` endpoints.

## Architecture

- **Two Rust crates**: root crate (`src/`) is both a library (`ai_gateway`) and a standalone binary (`ai-gateway`); `src-tauri/` is the Tauri desktop wrapper that depends on the root crate via `path = ".."`.
- **Frontend**: React + TypeScript + Mantine 7 + Vite in `frontend/`. Builds to `../static/` (NOT `frontend/dist/`), which the Rust binary serves as static files.
- **Database**: SQLite via rusqlite + r2d2. DB path resolves relative to app dir (`config.toml` location).
- **Config**: `config.toml` in app directory. Loaded via `AppConfig::load_or_default()` which searches upward from exe dir for `config.toml` (handles macOS .app bundles and Windows install paths).

## Build & run commands

```bash
# Standalone server mode (serves on port 1994)
cargo run

# Frontend dev server (port 5173, proxies /api /v1 /health to localhost:1994)
cd frontend && npm install && npm run dev

# Build frontend (outputs to ../static/)
cd frontend && npm run build

# Tauri desktop app
cargo install tauri-cli
cargo tauri dev

# Build release
cargo build --release
```

## Key ports

- **1994**: Rust backend (API + static file server). Configurable in `config.toml`.
- **5173**: Vite dev server (frontend only, proxies API calls to 1994).

## Tests

Unit tests live in `#[cfg(test)]` modules next to the code (notably `src/protocol/reasoning.rs`, `src/protocol/anthropic.rs`, `src/proxy/stream_convert.rs`, `src/proxy/reasoning_cache.rs`). Run them with `cargo test` (108 lib tests). CI (`release.yml`) only builds and creates GitHub releases — it does not run tests.

Browser E2E lives in `e2e/` (pytest + Playwright; extra deps are `axe-core` as a frontend devDependency and Pillow + numpy for the pixel diff): `python -m pytest e2e -q` (22 specs, ~82 s). Fixtures start isolated instances on dynamic ports with their own `config.toml`/`data/`, refuse to bind 1994 or 1995, and need `cargo build` plus `npm --prefix frontend run build` first. Harness rules that are not obvious from the code: real mouse clicks never reach React (click via `element.click()` in `page.evaluate`), Mantine modals are `[role=dialog]`, a hash-router navigation needs a fresh `?v=N` query to be a real document load, a stats row is only queryable ~500 ms after the response because StatsWriter batches writes, and `Console.goto(page, route, mount)` takes the mount selector per shell — `/wall` mounts `.wall-root`, not `.console-shell`.

`e2e/test_a11y.py` injects `frontend/node_modules/axe-core` into the page (`page.add_script_tag(path=...)`, no CDN) and asserts 0 WCAG 2.x A/AA violations on six pages (five console pages plus `/wall`) in both colour schemes. axe's `label-content-name-mismatch` normalizes NFKD and then **drops parenthesised content from the accessible name** — full-width CJK brackets included — so an accessible name like `通知中心（未读 1 条）` fails while `通知中心，未读 1 条` passes. Keep visible badge text outside brackets in the name.

`e2e/test_wall.py` covers `/wall` six times: data actually reaches the screen, a partial fetch failure is named on screen and recovers, the failed count drills down into an in-row error breakdown, the heatmap keeps its own range control (default 7 days, switching to 30 days refetches with a wider window), a 60-row request log expands with no inner vertical scroll, and a layout probe runs at 1280×800 / 1440×900 / 1920×1080 × dark/light (no horizontal page overflow, no top-level container outside the viewport, no readout clipped — `text-overflow: ellipsis` counts as intentional — grid track counts, heatmap cells not collapsed, exit button on screen), saving screenshots to `.dsh-tmp/wall/`. Two rules it encodes: derive expected responsive track counts from `window.matchMedia` inside the page, never from hardcoded pixel widths (1280 sits exactly on `.wall-pair`'s `max-width: 1279px`, and whether the scrollbar counts decides the branch); and recharts `YAxis` needs `allowDecimals={false}` or a `tickFormatter` that rounds produces duplicate axis labels (`1 / 1 / 1 / 0 / 0`) on small data. `test_matrix_failure_drilldown` stubs `model-summaries` with failures and `errors` with a scoped breakdown, then asserts the failed-count button expands an in-row detail (and that the request carries `model=`). The failure spec aborts two stats endpoints with `page.route` and asserts the `.wall-failbar` names exactly those blocks — naming every block would mean the wall collapsed, which is a different bug — and that aborting leaves `net::ERR_FAILED` resource logs behind, so assert "no pageerror", not `errors == []`.

`e2e/test_visual.py` is the pixel diff: three full-page shots (`/traffic`, `/settings`, `/wall`) against baselines in `.dsh-tmp/visual/` (gitignored; regenerate with `UPDATE_BASELINES=1 python -m pytest e2e/test_visual.py`). A per-pixel channel delta over 24 counts as different, and the failing ratio is 0.05% of pixels — that is only affordable because the screen is made fully deterministic: the visual instance pins its port (`Instance(name, port)`, port 1996) because the port is painted into the sidebar footer, the settings "管理端口" field and DB paths, and `page.clock.set_fixed_time()` stops the wall clock. Its seed DB holds one platform/proxy/key with fixed names and zero gateway requests. Two rules: a probe that is meant to prove the diff still catches a change must be written where it actually wins the cascade (a rule placed before the real same-specificity rule is silently overridden), and a size mismatch is reported as such rather than as "100% of pixels differ".

`e2e/test_keyboard.py` walks the keyboard: Tab across all six pages in both schemes, asserting each stop is interactive, has a visible focus ring (`outline` or `box-shadow`), and that focus really moved — detected by tagging `document.activeElement` before the Tab, not by comparing names, since two identically-named row buttons are both legitimate stops. Tabbing past the last stop hands focus to the browser (`activeElement === body`); that is the end of the tab order, not a failure, so the spec asserts "every stop before the end is valid" plus a minimum stop count per page. For the modal case, `focus()` the trigger before the synthetic `click()` — a synthetic click carries no focus, so "Esc returns focus to the trigger" would be unverifiable.

## Code conventions

- **Chinese comments** are used throughout the Rust backend. Keep comments in Chinese when editing existing files.
- **Rust style**: actix-web handlers, `AppError` enum with `From` impls, `AppResult<T>` type alias. Modules organized by domain: `api/`, `db/`, `proxy/`, `lb/`, `protocol/`.
- **API routes** registered in `src/api/mod.rs` under `/api` scope. Proxy endpoints at root level (`/v1/chat/completions`, `/v1/messages`, `/v1/models`).
- **Frontend**: React functional components, Mantine 7 components, i18n via `src/i18n.ts` (zh/en). Pages in `src/pages/`, API calls in `src/api.ts`.
- **Form fields use `components/Field.tsx`** (`<Field label=… unit=… full>`), which binds the label to the control with `useId()`. Do not hand-write `<div className="console-field"><label>…</label><Input/></div>` — a sibling label is invisible to screen readers and axe flags it.
- **Icon-only controls carry their own `aria-label`**: `ActionIcon`, `Pagination` (via `getControlProps` for `first`/`previous`/`next`/`last`) and unlabelled `Select`. A wrapping Mantine `Tooltip` only sets `aria-describedby` — it never contributes to the accessible name.
- **A failed fetch never renders as `0` or "no data"**: `/wall` keeps a per-block failure ledger (`failedRef` + one re-render tick, written by all three loaders), panels render an explicit "did not load" line instead of their empty state, and the readout strip renders `—` instead of a number when its block failed. Read from across a room, `0` and "could not load" look identical.
- **Every user-facing string goes through `t(locale, key)`** — no hardcoded labels, table headers, kickers or panel subs. Chinese mode keeps only proper nouns and units in Latin script (AI Gateway, Key, Token, API Key, OpenAI, Base URL, cURL, JSON, `max_tokens`, `(ms)`); enum values keep their API spelling in state and are localised at render (`optionLabel()` in `ModelDetail.tsx`).
- **Type floor is 11px**; body/table text is 12–13px. Uppercase mono eyebrows earn their hierarchy from weight, colour and tracking, never from shrinking below 11px — `console.css` had 51 rules at 8px before this was fixed.
- **No linting/formatting tools** configured for Rust or TypeScript (no clippy, rustfmt, eslint, prettier).
- **Dev profile**: `opt-level = 2` for all dependencies (faster dev builds at cost of compile time).

## Reasoning / extended thinking

- Unified model lives in `src/protocol/reasoning.rs` (`ReasoningConfig`, `ReasoningEffort`, `ReasoningStyle`, `ReasoningTarget`).
- Inbound: OpenAI `reasoning_effort`, OpenRouter-style `reasoning`, and vendor fields (`enable_thinking`, `thinking_budget`, `chat_template_kwargs`); Anthropic top-level `thinking`.
- Outbound: `backend.reasoning_style` (SQLite migration V9) selects the wire format; `Auto` resolves by platform type (Anthropic → `thinking`, others → `reasoning_effort`). `param_overrides` merges last and wins.
- Same-protocol non-stream responses are passed through raw after rewriting `model`; cross-protocol responses are rebuilt from the unified format.
- Anthropic thinking signatures must be replayed unchanged. Cross-protocol tool loops use the in-memory cache in `src/proxy/reasoning_cache.rs`, keyed by `tool_use.id` (30 min TTL, bound to backend/model, fail-open by disabling thinking on miss).
- Synthetic signatures (`gw_sig_*`) are for Anthropic clients only; `anthropic::to_request` strips them before forwarding to a real Anthropic upstream.

## Admin authentication

- Admin login lives in `src/api/auth.rs`. The `/api` scope is wrapped by session middleware; only `/api/auth/status`, `/api/auth/login` and `/api/auth/setup` are public. First-time setup is restricted to loopback (127.0.0.1).
- Credentials are stored in SQLite (`users`, migration V10, Argon2id hashes). Sessions are rows in `sessions` plus an HttpOnly `aigw_session` cookie (7 days).
- AI endpoints are outside the admin session: `/v1/models` is public; chat/completions/messages authenticate with gateway API keys (`api_keys` table).
- Forgot password: delete rows from `users` and `sessions`, restart, and run setup again.

## Gotchas

- Frontend builds to `../static/` not `frontend/dist/` — the `emptyOutDir: true` in `vite.config.ts` means `static/` gets wiped on each frontend build.
- **`src-tauri/` is not covered by `cargo test` or the E2E fixtures**, so it drifts silently: the 2026-10-22 deploy found `ProxyState.config` still taking a plain `AppConfig` after it became a shared `Arc<RwLock<_>>`, plus a missing SPA fallback. Run `cargo check -p ai-gateway-tauri` (or a full `cargo tauri build --no-bundle`) after touching server setup in the root crate, and don't pipe the build command into `tail` — that hides a failed exit code.
- **Mantine `autoContrast` picks the wrong shade in dark mode**: the filled-variant colour resolver evaluates lightness against the *light* `primaryShade` (`brand[6]` = #2E63B8), so it keeps white text even though the dark scheme paints `brand[4]` (#5C9DFF) — 2.7:1. `main.tsx` overrides `variantColorResolver` so brand-filled surfaces use the `--accent-ink` token (dark ink in dark mode, white in light). Do not "fix" this by darkening `--accent` itself; it is also the text/link colour on dark panels.
- **Stats time windows are second-precision string comparisons**: `db/stats.rs::normalize_bound` rounds a `from`/`to` bound with a fractional part **up** to the next second. `request_stats.created_at` is stored as RFC3339 with nanoseconds and the SQL compares it lexicographically (`created_at < to`), so truncating the bound to seconds drops every row written inside that second — with the Live range (`to = now`) the newest request becomes unqueryable for up to a second. Keep new time filters on the same helper.
- The Rust binary finds static files and config by searching upward from the executable directory for `config.toml`. This path resolution is critical for Tauri bundles on macOS (Contents/Resources/_up_/).
- Config resolution searches upward from exe dir. On Windows it may find `C:\Users\<user>\AppData\Local\AI Gateway\config.toml` before the project-local one — that becomes the active config and DB path.
- The `config.toml` `encrypt_key` and `admin_token` are security-sensitive. Never commit real values.
- No `data/` directory or `*.db` files should be committed (gitignored).
- Anthropic extended thinking requires `budget_tokens >= 1024` and `max_tokens > budget_tokens`, and rejects `temperature` / `top_p` / `top_k`; `anthropic::to_request` clamps the budget (never raises `max_tokens`) and strips those sampling params when thinking is enabled.
- Thinking content and signatures are never logged; only parameter shapes and cache hit/miss are recorded at debug level.

