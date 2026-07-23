---
description: Build Windows installer (MSI + NSIS) via cargo tauri build
---

# Tauri Windows Build

Build the ai-gateway Tauri desktop app for Windows, producing MSI and NSIS installers.

## Usage

```
tauri-build
```

## Steps

1. **Ensure frontend is built first** (static files go to `../static/` relative to `src-tauri/`)
   ```bash
   cd D:\item\ai-gateway\frontend && npm run build
   ```

2. **Build the Tauri app**
   ```bash
   cd D:\item\ai-gateway\src-tauri && cargo tauri build 2>&1
   ```
   Timeout: 600000ms (10 minutes — first build compiles everything from scratch)

3. **Output location**
   - Installers: `src-tauri/target/release/bundle/`
   - Binary: `src-tauri/target/release/ai-gateway.exe`

## Common errors

- **`unresolved import`** — root crate source files not committed. Run `git add src/` and commit before building.
- **`.menu(menu)` type error** — Tauri 2.x expects `&menu`. Fix: `.menu(&menu)`.
- **`on_tray_icon_event` unused variable** — change `|tray, event|` to `|_tray, event|`.
- **Stale static files** — always rebuild frontend before Tauri build. The `emptyOutDir: true` in `vite.config.ts` wipes `../static/` on each build.

## Notes

- `cargo tauri build` compiles both the root crate and `src-tauri` crate
- First build takes 5-10 minutes; incremental builds are faster
- Output goes to `src-tauri/target/release/bundle/` (NOT `target/release/bundle/`)
- For CI/CD: GitHub Actions `release.yml` handles this automatically on tag push
