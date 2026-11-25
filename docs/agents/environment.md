# Agent Environment & Sandbox Notes

Non-obvious local toolchain and sandbox behavior discovered while running checks on macOS.

## Frontend Toolchain (`npm`)

- The workspace uses **`npm`** (Node 22, committed `web/package-lock.json`); `pnpm` is not installed.
- Fresh `git worktree` checkouts lack `web/node_modules`. Hydrate it offline inside the sandbox before running web checks or `scripts/check.sh`:

```bash
npm ci --offline --prefix web
```

## macOS Sandbox Boundaries

- **Avoid `/usr/bin/make` inside the sandbox**: `/usr/bin/make` triggers macOS's `xcode-select` shim before reading `Makefile` and fails with `/var/db/xcode_select_link: Operation not permitted`. Invoke `./scripts/check.sh --sandbox`, `npm --prefix web ...`, and `cargo ...` directly inside the sandbox; reserve `make check` / `make test-server` for `BypassSandbox: true`.
- **Pre-commit & sandbox checks**: `.githooks/pre-commit` runs `./scripts/check.sh --sandbox`, covering Rust lint/tests, web checks/asset gates, `mahjong-core` + `mahjong-ai` tests, non-socket `mahjong-server` tests, boundary/license audits, and glossary checks inside the sandbox.
- **Full suite & live sockets**: `make check` / `make test-server` run `mahjong-server` HTTP/WS integration tests binding `127.0.0.1`. When the macOS sandbox blocks localhost socket connections (`Operation not permitted`), re-run that test command alone with `BypassSandbox: true`.
- **Network / `.git` locks**: Isolate `cargo fetch`, `npm install`, `gh`, and main-repo `git` write lock failures (`Operation not permitted` on `.git/*.lock`) with `BypassSandbox: true` by themselves rather than elevating build/test chains.
- **Sandbox CLI & media tools**: Prefer `grep` and `node` over `rg` or `python3` in ad-hoc sandboxed commands; `scripts/check.sh` auto-sets `DEVELOPER_DIR` for Xcode CLT. For local asset builds, `/usr/bin/sips` cannot rasterize SVG (use `swift` CoreGraphics/CoreText or Node.js), while `/usr/bin/afconvert` and `/usr/bin/afinfo` handle `.m4a` encoding and inspection.
- **File reading in worktrees**: Pass absolute paths rooted at the worktree `pwd` to file-reading tools.
