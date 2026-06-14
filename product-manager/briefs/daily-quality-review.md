# Daily Quality Review — 2026-06-14

> **Path note:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules not installed).
- 0 tests run. Previous: 0 run (2026-06-08). Persistent — day 6.
- **Action:** `npm ci` in `directing/clearing`.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules.
- Persistent since 2026-06-06 — day 8.
- **Action:** `npm ci` at repo root.

## Build (`directing/clearing` TypeScript)
**RED** — 149 type errors (`npx tsc --noEmit`).
- Previous: 140 errors (2026-06-08). **+9 new errors — active regression.**
- **Action:** Investigate new type errors; check recent commits to `directing/clearing`.

## Board-Client
**N/A** — `messages/board-client` not in repo; no substitute this cycle.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 62/62 pass (2026-06-06). 0 run — day 6.
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 52/52 pass (2026-06-06). 0 run — day 6.
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 69/69 pass (2026-06-06). 0 run — day 6.
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data collected. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN.

## Failure Delta (vs 2026-06-08)
- **PERSISTENT:** All 4 test suites BLOCKED (ts-jest missing) — no progress in 6 days.
- **PERSISTENT:** Lint RED (@eslint/js missing) — no progress in 8 days.
- **NEW REGRESSION:** Build errors 140 → 149 (+9). Type-breaking changes landed while tests are dark.
- **Priority:** node_modules fix is a prerequisite for everything else; build regression is independently actionable now.
