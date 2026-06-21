# Daily Quality Review — 2026-06-21

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 13.
- 0 tests run. No change from 2026-06-20.
- **Action:** `npm ci` in `directing/clearing`.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 15.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root.

## Build (`directing/clearing` TypeScript)
**RED** — 150 type errors (`npx tsc --noEmit`). **+1 regression from yesterday (149).**
- Previous: 149 (2026-06-20). One new type error introduced today.
- **Action:** Investigate new type error — regression needs attention.

## Board-Client
**N/A** — No equivalent in repo this cycle.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 13.
- Previous: 62/62 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 13.
- Previous: 52/52 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 13.
- Previous: 69/69 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-06-20)
- **REGRESSION:** Build type errors 149 → 150 (+1). New error introduced today.
- **UNCHANGED:** All 4 suites blocked (ts-jest) — day 13, no progress since 2026-06-08.
- **UNCHANGED:** Lint blocked (@eslint/js) — day 15, no progress since 2026-06-06.
- **Root cause:** All node_modules empty across every package. Single `npm ci` pass at root + each package resolves 183 dark tests and lint in one sweep. **Day 13 unresolved + new build regression — escalation critical.**
