# Coding Standards (`majjj`)

Judgement-call standards for code review across the workspace. Mechanical checks (formatting, `todo!`/`dbg!` bans, `mahjong-core` & `mahjong-ai` boundary/purity/license audits, `ts-rs` zero-drift & no-`bigint` checks, `web` Safari/hint safeguards, and `GLOSSARY.md` synonym bans) are enforced automatically by `./scripts/check.sh` (`make check`).

## Privacy Projection Boundary (`mahjong-core`)

- Keep `TableState`, `TableEvent`, and hidden reaction windows (`AwaitingDiscardReactions`, `AwaitingRobKong`) crate-private; project external snapshots and increments solely via `project_view` and `project_event`.
- Keep `PublicPhase` at `Playing` while waiting on reactions; keep `legal_actions` empty for non-eligible seats; project opponent `ConcealedKong` as `tile: None` and opponent `TileDrawn` as `tile: None` until `HandSettled`.

## Minimal Protocol & Cohesive Session Boundary

- Keep crate-internal error enums (`MatchError`) limited to variants actually produced and consumed at that crate layer; reserve forward-compatible wire/contract variants in `mahjong-server` / `mahjong-ai` only when explicitly frozen in `docs/specs/v1-architecture-and-design.md`.
- Keep `TableSession` responsibilities cohesive (extract focused modules/helpers for match driving, confirmation ops, and connection state rather than letting `session.rs` accumulate unrelated concerns).
- Ensure every `SubmitAction` rejection path (including invalid phase or unseated caller) emits `ActionRejected` or clears the client input lock so the UI never remains stuck waiting on a receipt.

## Cohesive Engine State, Frontend & Public-Seam TDD

- Encapsulate per-seat mutations (discarding, claiming, consuming tiles, declaring melds, recording over-water locks) as methods on `SeatState` rather than mutating fields across engine modules.
- Share reaction-window pipelines (`AwaitingDiscardReactions`, `AwaitingRobKong`) rather than duplicating phase extraction and resolution boilerplate.
- Keep seat/tile/meld/settlement/config metadata (`web/src/lib/protocol/` — `seats.ts`, `melds.ts`, `settlements.ts`) and layout margins single-sourced in `web/src/lib/` rather than duplicating switch tables or geometry formulas across components and canvas renderers.
- Write tests strictly against public seams (`Match`, `Ruleset`, `WallProvider`, `decide`, HTTP/WS endpoints, `ts-rs` export, and pure TS session reducer) with independent spec-derived expectations, sharing crate-local test boilerplate in `tests/common/mod.rs`.
