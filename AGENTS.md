## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues (`highing666/majjj`) via the `gh` CLI (requires `BypassSandbox: true`). To read a spec and its sub-issue task graph without truncation, run `./scripts/fetch-spec-graph.sh <spec-number> <out.md>`. See `docs/agents/issue-tracker.md` for all conventions.

### Triage labels

Uses the default five canonical triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout (`GLOSSARY.md` and `docs/adr/` at the repo root). See `docs/agents/domain.md`.

### Coding standards

Judgement-call review standards live in `CODING_STANDARDS.md` (consult during code review).

### Environment & sandbox

Shell search (`grep`/`node`, not `rg`), macOS sandbox `cargo`/`git` caveats, and worktree `npm` setup live in `docs/agents/environment.md`.

### Architecture spec

Authoritative v1 rules, protocol, AI, AV, and delivery slices: `docs/specs/v1-architecture-and-design.md`.

### Verification

Run `./scripts/check.sh` (`make check`) before finishing. In sandboxed shells where localhost sockets or `/usr/bin/make` are restricted, run `./scripts/check.sh --sandbox` plus `make test-server` (`BypassSandbox: true`), and hydrate fresh worktrees with `npm ci --offline --prefix web`. After marking a PR ready, run `gh pr checks <pr>` (`BypassSandbox: true`) to confirm Linux CI parity.
