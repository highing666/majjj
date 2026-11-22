## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues (`highing666/majjj`) via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five canonical triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout (`GLOSSARY.md` and `docs/adr/` at the repo root). See `docs/agents/domain.md`.

### Coding standards

Judgement-call review standards live in `CODING_STANDARDS.md` (consult during code review).

### Environment & sandbox

Frontend `npm` worktree setup and macOS sandbox caveats live in `docs/agents/environment.md`.

### Architecture spec

Authoritative v1 rules, protocol, AI, AV, and delivery slices: `docs/specs/v1-architecture-and-design.md`.

### Verification

Run `./scripts/check.sh` (`make check`) before finishing (`fmt`, `clippy`, `web` typecheck/test/build, workspace tests + `ts-rs` drift, boundaries/license audit, web safeguards, and glossary checks). Use `./scripts/check.sh --sandbox` (`make check-sandbox`) + `make test-server` when localhost sockets are sandbox-restricted; run `make web-install` (or `npm ci --offline --prefix web`) in fresh worktrees.
