---
name: frontend-rebuild
description: Rebuild frontend static files and restart backend to apply frontend changes
---

# Frontend Rebuild & Backend Restart

After editing any file under `frontend/src/`, the changes must be compiled into `../static/` and the Rust backend must be restarted to serve them.

## When to use

- After modifying any `.tsx`, `.ts`, `.css`, or `package.json` in `frontend/`
- After switching between dev mode (`npm run dev` on port 5173) and production mode
- When frontend changes aren't appearing despite saving files

## Steps

1. **Build frontend** (outputs to `../static/`)
   ```bash
   cd D:\item\ai-gateway\frontend && npm run build
   ```

2. **Restart Rust backend** (serves static files on port 1994)
   ```bash
   # Kill existing process
   Stop-Process -Name "ai-gateway" -Force -ErrorAction SilentlyContinue
   Start-Sleep -Seconds 2
   # Start new process
   Start-Process -FilePath "D:\item\ai-gateway\target\debug\ai-gateway.exe" -WorkingDirectory "D:\item\ai-gateway"
   ```

3. **Verify** — open `http://localhost:1994` and check changes are reflected

## Key gotchas

- **`static/` gets wiped on every `npm run build`** — `emptyOutDir: true` in `vite.config.ts` means the entire `static/` directory is cleaned before each build. Never put manual files there.
- **Dev server (port 5173) is separate** — `npm run dev` runs Vite on 5173 with hot reload and proxies API calls to 1994. This is for development only. Production mode serves from the Rust binary on 1994.
- **Backend serves static from `../static/`** — the Rust binary resolves the static path relative to the `config.toml` location, which searches upward from the exe directory. When running from `target/debug/`, it finds the project-root `config.toml` first.
- **Do not modify `static/` manually** — it's a build artifact. All changes go through `frontend/src/` → `npm run build`.

## Frontend dev mode alternative

For rapid iteration without restarting the backend:
```bash
cd D:\item\ai-gateway\frontend && npm run dev
```
This starts Vite on port 5173 with hot module replacement. API calls are proxied to localhost:1994 automatically. Access at `http://localhost:5173`.
