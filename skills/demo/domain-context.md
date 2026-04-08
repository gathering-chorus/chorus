# Demo Domain Context

**Owner:** Wren
**Product:** /demo skill — proving gate ceremony
**Domain:** Demo lifecycle — how work gets shown, validated, and accepted

## What Demo Does
Enforces DEC-048: no self-service Done for code cards. Builder shows the work, Jeff and Wren validate, then accept or reject. The ceremony catches "done but not done" before Jeff sees it.

## Gates (owned by this product)
- **preflight.sh** — card in WIP, AC exists, smoke check passes, ICD renders
- **done-gate.sh** — demo evidence required before Done (skip for chore/swat)
- **provenance.sh** — generates demo brief, emits spine event, records on board

## Spine Events Emitted
- `card.demo.started` — demo ceremony initiated
- `card.accepted` — card accepted after demo
- `card.rejected` — card rejected with reason

## Dependencies
- Cards CLI (`platform/scripts/cards`) — card status, AC parsing
- Smoke check (`platform/scripts/smoke-check.sh`) — page health
- Chorus API (localhost:3340) — spine event search for demo evidence
- chorus-hooks — dispatches to gate scripts, returns allow/deny

## Contract
Gate scripts exit 0 (allow) or 1 (deny, stderr = message). chorus-hooks dispatches and maps exit codes. No gate logic in hooks — all logic in skills/demo/gates/.
