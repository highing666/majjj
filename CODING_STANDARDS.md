# Coding Standards (`majjj`)

Judgement-call standards for code review across the workspace. Mechanical checks (formatting, `todo!`/`dbg!` bans, `mahjong-core` stdlib I/O/clock/async bans, `ts-rs` drift, and `GLOSSARY.md` synonym bans) are enforced automatically by `./scripts/check.sh` (`make check`).

## Privacy Projection Boundary (`mahjong-core`)

- Keep `TableState`, `TableEvent`, and hidden reaction windows (`AwaitingDiscardReactions`, `AwaitingRobKong`) crate-private; project external snapshots and increments solely via `project_view` and `project_event`.
- Keep `PublicPhase` at `Playing` while waiting on reactions; keep `legal_actions` empty for non-eligible seats; project opponent `ConcealedKong` as `tile: None` and opponent `TileDrawn` as `tile: None` until `HandSettled`.

## Minimal Protocol & JSON/TS Fidelity

- Keep `MatchError` and protocol enums limited to variants actually produced and consumed at that crate layer; leave session/transport validation to `mahjong-server`.
- Annotate `u64` protocol fields serialized over `serde_json` with `#[ts(type = "number")]` so generated TypeScript bindings match JavaScript `JSON.parse` values instead of emitting `bigint`.

## Cohesive Engine State & Public-Seam TDD

- Encapsulate per-seat mutations (discarding, claiming, consuming tiles, declaring melds, recording over-water locks) as methods on `SeatState` rather than mutating fields across engine modules.
- Share reaction-window pipelines (`AwaitingDiscardReactions`, `AwaitingRobKong`) rather than duplicating phase extraction and resolution boilerplate.
- Write tests strictly against public seams (`Match`, `Ruleset`, `WallProvider`, `project_view`/`project_event`, `ts-rs` export) with independent spec-derived expectations, using `tests/common::tiles(...)` shorthand for readable hand fixtures.
