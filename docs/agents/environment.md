# Agent Environment & Sandbox Notes

Non-obvious local toolchain and sandbox behavior discovered while running checks on macOS.

## Frontend Toolchain (`npm`)

- The workspace uses **`npm`** (Node 22, committed `web/package-lock.json`); `pnpm` is not installed.
- Fresh `git worktree` checkouts lack `web/node_modules`. Hydrate it offline inside the sandbox before running web checks or `scripts/check.sh`:

```bash
npm ci --offline --prefix web
```

## macOS Sandbox Boundaries

- **Standalone `cargo` invocations**: Prefix direct `cargo` commands inside the sandbox with `DEVELOPER_DIR=/Library/Developer/CommandLineTools` so linking does not hit `/var/db/xcode_select_link` (`scripts/check.sh` and `Makefile` already export it).
- **Avoid `/usr/bin/make` inside the sandbox**: `/usr/bin/make` triggers macOS's `xcode-select` shim before reading `Makefile` and fails with `/var/db/xcode_select_link: Operation not permitted`. Invoke `./scripts/check.sh --sandbox`, `npm --prefix web ...`, and `DEVELOPER_DIR=/Library/Developer/CommandLineTools cargo ...` inside the sandbox; reserve `make check` / `make test-server` / `make dist-mac` for `BypassSandbox: true`.
- **Pre-commit & sandbox checks**: `.githooks/pre-commit` runs `./scripts/check.sh --sandbox --staged` (scoped to staged files for fast commits). Name socket-free `mahjong-server` integration tests `*_virtual_clock.rs` or `*_offline.rs` so `./scripts/check.sh --sandbox` auto-discovers them. During iterative frontend work, run `npm --prefix web run typecheck && npm --prefix web test`, then `./scripts/check.sh --sandbox --since <base>`; during iterative Rust socket-test work, run `DEVELOPER_DIR=/Library/Developer/CommandLineTools cargo test -p mahjong-server --test <name>` (`BypassSandbox: true`) without rebuilding Vite, reserving full `./scripts/check.sh --sandbox` plus `make test-server` (`BypassSandbox: true`) for final pre-PR verification.
- **Glossary gate**: `./scripts/check-glossary.sh` scans raw text including comments and `*.test.ts` files; rely on the script to ban synonyms instead of embedding banned tokens inside `.not.toMatch(...)` regexes.
- **Code review economy**: Before spawning parallel `research` review subagents on a large branch, write `git diff <base>...HEAD -- . ':(exclude)web/assets-src/**' ':(exclude)web/src/assets/atlas/**' ':(exclude)web/src/lib/protocol/generated/**' > <scratch>/review.diff` once as a context pointer, and let built-in `research` subagents return reports directly instead of calling `send_message`.
- **Full suite & live sockets**: `make check` / `make test-server` run `mahjong-server` HTTP/WS integration tests binding `127.0.0.1`. When the macOS sandbox blocks localhost socket connections (`Operation not permitted`), run that test command alone with `BypassSandbox: true`.
- **All `git` writes (main repo & `share` worktrees) & network ops**: `Workspace: "share"` worktrees write locks and refs into `<main-repo>/.git/worktrees/`, so EVERY `git` write (`checkout`, `add`, `commit`, `merge`, `branch -d`, `worktree prune`) and network command (`cargo fetch`, `npm install`, `gh`) requires `BypassSandbox: true` by itself rather than elevating build/test chains. Keep each `BypassSandbox: true` `git` / `gh` command unchained (one command per tool call, no `&&`) so prefix allowlisting auto-approves unattended parallel runs. Always pass `--no-edit` (or `-m`) to `git merge` because `GIT_EDITOR=false`.
- **Worktree cleanup**: After `manage_subagents` kills subagents, run `git worktree prune` (`BypassSandbox: true`) to clear removed worktree metadata before deleting temporary branches.
- **Sandbox CLI tools**: Use `grep` and `node` for ad-hoc shell searches and scripts (`rg` exits 127 and `/usr/bin/python3` triggers macOS's `xcode-select` shim in the sandbox).
- **Local asset tools**: `/usr/bin/sips` cannot rasterize SVG; invoke `/Library/Developer/CommandLineTools/usr/bin/swift -module-cache-path <workspace-dir>` with `DEVELOPER_DIR=/Library/Developer/CommandLineTools`, `CLANG_MODULE_CACHE_PATH`, and `SWIFT_MODULE_CACHE_PATH` pointing inside the workspace (see [web/scripts/build-atlas.mjs](file:///Users/highing/code/majjj/web/scripts/build-atlas.mjs)). `/usr/bin/afconvert` and `/usr/bin/afinfo` handle `.m4a` encoding and inspection.
- **File reading in worktrees**: Pass absolute paths rooted at the worktree `pwd` to file-reading tools.
