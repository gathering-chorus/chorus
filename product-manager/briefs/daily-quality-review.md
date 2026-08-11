# Daily Quality Review — 2026-08-11

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 61.
- 0 tests run. No change from 2026-08-10.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 63.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 202 type errors. **UNCHANGED vs yesterday (was 202).**
- No new regressions today.
- **Action:** 202 errors remain. Steady-state since 2026-08-10.

## Board-Client
**N/A** — No `messages/board-client` package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 61.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 61.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 61.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-10)
- **STABLE:** Build type errors held at 202 (no new regression today).
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 61.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 63.
- **Primary blocker:** `npm ci` at repo root. **63 days unresolved.**
