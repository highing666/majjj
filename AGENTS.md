## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues (`highing666/majjj`) via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five canonical triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout (`GLOSSARY.md` and `docs/adr/` at the repo root). See `docs/agents/domain.md`.

### Architecture spec

Authoritative v1 rules, protocol, AI, AV, and delivery slices: `docs/specs/v1-architecture-and-design.md`.

### Verification

Run `./scripts/check.sh` (or `make check`) before committing (`fmt`, `clippy`, workspace tests, `ts-rs` drift, `mahjong-core` boundary, and glossary checks; auto-sets macOS `DEVELOPER_DIR` for sandboxed runs).
