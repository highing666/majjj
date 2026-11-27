# Coding Standards (`majjj`)

Judgement-call standards for code review across the workspace. Mechanical checks (formatting, `todo!`/`dbg!` bans, `mahjong-core` & `mahjong-ai` boundary/purity/license audits, `ts-rs` zero-drift & no-`bigint` checks, `web` Safari/hint safeguards, and `GLOSSARY.md` synonym bans) are enforced automatically by `./scripts/check.sh` (`make check`).

## Privacy Projection Boundary (`mahjong-core`)

- Keep `TableState`, `TableEvent`, and hidden reaction windows (`AwaitingDiscardReactions`, `AwaitingRobKong`) crate-private; project external snapshots and increments solely via `project_view` and `project_event`.
- Keep `PublicPhase` at `Playing` while waiting on reactions; keep `legal_actions` empty for non-eligible seats; project opponent `ConcealedKong` as `tile: None` and opponent `TileDrawn` as `tile: None` until `HandSettled`.

## Minimal Protocol & Cohesive Session Boundary

- Keep crate-internal error enums (`MatchError`) limited to variants actually produced and consumed at that crate layer; reserve forward-compatible wire/contract variants in `mahjong-server` / `mahjong-ai` only when explicitly frozen in `docs/specs/v1-architecture-and-design.md`.
- Keep `TableSession` responsibilities cohesive (extract focused modules/helpers for match driving, confirmation ops, and connection state rather than letting `session.rs` accumulate unrelated concerns).
- Ensure every `SubmitAction` rejection path (including invalid phase or unseated caller) emits `ActionRejected` or clears the client input lock so the UI never remains stuck waiting on a receipt.
- Decouple bounded gameplay sync waits (e.g., highlight short-sync caps) and upstream queue-admission deadlines (e.g., TTS `deadline_ms`) from underlying in-flight async HTTP request timeouts so slow-but-healthy responses unblock gameplay and complete cleanly without aborting active GPU/LLM work or firing false outage notices.
- Pair WebSocket credential verification and revocation-channel subscription atomically before `ws.on_upgrade`, and treat any revocation signal unconditionally as a terminal disconnect (never gating disconnect on mutable `is_enabled()` state, which races with fast disable/re-enable).

## Cohesive Engine State, Frontend & Public-Seam TDD

- Encapsulate per-seat mutations (discarding, claiming, consuming tiles, declaring melds, recording over-water locks) as methods on `SeatState` rather than mutating fields across engine modules.
- Share reaction-window pipelines (`AwaitingDiscardReactions`, `AwaitingRobKong`) rather than duplicating phase extraction and resolution boilerplate.
- Keep seat/tile/meld/settlement/config metadata (`web/src/lib/protocol/` — `seats.ts`, `melds.ts`, `settlements.ts`), layout margins, and inner canvas widget metrics (badge pill/name budgets, compass ring radii) single-sourced in `web/src/lib/` rather than duplicating switch tables or geometry formulas across components and canvas renderers.
- Wire every blocking `role="dialog"` / `aria-modal="true"` overlay into `hasModalOpen` so global and action shortcuts (`F`, `H`, `G`, `P`, `Esc`) are suppressed underneath, and keep native vs. fallback platform controller exit semantics identical (e.g., `Esc` exit and shortcut grace windows).
- Verify cross-phase HUD overlays (banter bubbles, status notices) remain visible across `InMatch → BetweenHands → Finished` instead of being covered by modal backdrops or unmounted prematurely on phase transitions, and verify host/global in-match controls remain reachable in both seated (`MatchHud`) and unseated observer sub-views.
- Exercise worst-case table occupancy (2–4 melds per seat and full river rows) across compact landscape (`844×390`, `568×320`) and portrait fallback viewports in layout zero-overlap test matrices.
- Write tests strictly against public seams (`Match`, `Ruleset`, `WallProvider`, `decide`, HTTP/WS endpoints, `ts-rs` export, and pure TS session reducer) with independent spec-derived expectations, sharing crate-local test boilerplate in `tests/common/mod.rs` and frontend table test fixtures in `web/src/lib/table/testFixtures.ts`.
