# Daily Quality Review — 2026-06-20

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 12.
- 0 tests run. No change from 2026-06-18.
- **Action:** `npm ci` in `directing/clearing`.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 14.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root.

## Build (`directing/clearing` TypeScript)
**RED** — 149 type errors (`npx tsc --noEmit`).
- Previous: 149 (2026-06-18). Stable — no new regressions, no fixes.
- **Action:** Type errors are independent of node_modules; fix is actionable now.

## Board-Client
**N/A** — No equivalent in repo this cycle.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 12.
- Previous: 62/62 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 12.
- Previous: 52/52 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules empty). Day 12.
- Previous: 69/69 pass (2026-06-06). No change.
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-06-18)
- **UNCHANGED:** All 4 suites blocked (ts-jest) — day 12, no progress since 2026-06-08.
- **UNCHANGED:** Lint blocked (@eslint/js) — day 14, no progress since 2026-06-06.
- **UNCHANGED:** Build 149 type errors — stable, no regressions or fixes.
- **Root cause:** All node_modules empty across every package. `npm ci` at root + each package resolves tests and lint in one pass. **Day 12 unresolved — 183 tests dark, escalation critical.**
