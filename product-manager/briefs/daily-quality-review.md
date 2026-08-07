# Daily Quality Review — 2026-08-07

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 57.
- 0 tests run. No change from 2026-08-06.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 59.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 187 type errors. **REGRESSION: +6 vs yesterday (was 181).**
- **Action:** New type errors introduced since 2026-08-06. Investigate recent commits.

## Board-Client
**N/A** — No `messages/board-client` package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 57.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 57.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 57.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-06)
- **NEW REGRESSION:** Build type errors rose 181 → 187 (+6). First increase in 14 days.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 57.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 59.
- **Primary blocker remains:** `npm ci` at repo root. Now **59 days unresolved.**
