# Agent Environment & Sandbox Notes

Non-obvious local toolchain and sandbox behavior discovered while running checks on macOS.

## Frontend Toolchain (`npm`)

- The workspace uses **`npm`** (Node 22, committed `web/package-lock.json`); `pnpm` is not installed.
- Fresh `git worktree` checkouts lack `web/node_modules`. Hydrate it offline inside the sandbox before running web checks or `scripts/check.sh`:

```bash
npm ci --offline --prefix web
```

## macOS Sandbox Boundaries

- **Pre-commit & sandbox checks**: `.githooks/pre-commit` runs `./scripts/check.sh --sandbox` (`make check-sandbox`), which runs `fmt`, `clippy`, `web` checks (`typecheck`/`test`/`build`), `mahjong-core` + `mahjong-ai` tests, non-socket `mahjong-server` tests, boundary/license checks, web safeguards, and glossary checks inside the sandbox.
- **Full suite & live sockets**: `make check` / `make test-server` run `mahjong-server` HTTP/WS integration tests binding `127.0.0.1`. When the macOS sandbox blocks localhost socket connections (`Operation not permitted`), re-run that test command alone with `BypassSandbox: true`.
- **Network / `.git` locks**: Isolate `cargo fetch`, `npm install`, `gh`, and main-repo `git` write lock failures (`Operation not permitted` on `.git/*.lock`) with `BypassSandbox: true` by themselves rather than elevating build/test chains.
- **Sandbox CLI tools**: Prefer `grep` and `node` over `rg` or `python3` in ad-hoc sandboxed commands; `Makefile` and `scripts/check.sh` auto-set `DEVELOPER_DIR` for Xcode CLT.
- **File reading in worktrees**: Pass absolute paths rooted at the worktree `pwd` to file-reading tools.
