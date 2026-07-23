---
name: version-release
description: Bump version across all files, commit, tag, and trigger GitHub release with build artifacts
---

# Version Bump & GitHub Release

Workflow for updating the ai-gateway version number across the entire codebase and publishing a release on GitHub.

## When to use

- User requests a version bump (e.g. "版本号改成1.2.5", "提交release")
- After significant fixes or features are ready for distribution
- User says "让GitHub打包" or "release"

## Files that contain version numbers

| File | Format | Example |
|------|--------|---------|
| `Cargo.toml` | `version = "X.Y.Z"` | `version = "1.2.4"` |
| `src-tauri/tauri.conf.json` | `"version": "X.Y.Z"` | `"version": "1.2.4"` |
| `src-tauri/Cargo.toml` | `version = "X.Y.Z"` (may lag) | check manually |
| `frontend/src/App.tsx` | `vX.Y.Z` in header text | `<Text>v1.2.4</Text>` |
| `.github/workflows/release.yml` | `## What's New in vX.Y.Z` | release notes header |
| `README.md` / `README_EN.md` | version references | check manually |
| `Cargo.lock` files | `version = "X.Y.Z"` for `ai-gateway` | regenerate via `cargo generate-lockfile` |

## Steps

1. **Find all version references**
   ```bash
   cd D:\item\ai-gateway
   rg --no-heading "1\.2\.[0-9]" --include "*.toml" --include "*.json" --include "*.tsx" --include "*.yml" --include "*.md" .
   ```

2. **Edit each file** — replace old version with new version in all locations listed above

3. **Regenerate lockfiles**
   ```bash
   cargo generate-lockfile
   # Also regenerate src-tauri/Cargo.lock if it exists
   cd src-tauri && cargo generate-lockfile
   ```

4. **Rebuild frontend** (to embed new version in static files)
   ```bash
   cd frontend && npm run build
   ```

5. **Verify** — check no old version references remain
   ```bash
   rg --no-heading "OLD_VERSION" .
   ```

6. **Git commit + tag + push**
   ```bash
   git add -A
   git commit -m "vX.Y.Z: <description of changes>"
   git tag vX.Y.Z
   git push origin main --tags
   ```

7. **Create GitHub release** (triggers CI to build Windows/macOS/Linux packages)
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes "release notes"
   ```
   Note: `release.yml` already contains bilingual release notes. Update them before tagging if needed.

8. **Verify CI triggered**
   ```bash
   gh run list --limit 3
   ```

## Gotchas

- `Cargo.lock` files contain the version for the `ai-gateway` package — forget to update them and CI build will fail with version mismatch
- `src-tauri/Cargo.lock` is separate from root `Cargo.lock` — both need updating
- Frontend must be rebuilt (`npm run build`) before tagging so the static files in `../static/` have the new version
- GitHub release CI builds both Windows (MSI + NSIS) and macOS packages — no manual `cargo tauri build` needed if CI is configured
- If CI fails due to missing source files (like `cache.rs` not being committed), fix and force-push the tag: `git tag -d vX.Y.Z && git tag vX.Y.Z && git push origin -f vX.Y.Z`
