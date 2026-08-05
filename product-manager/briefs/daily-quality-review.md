# Daily Quality Review — 2026-08-05

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 55.
- 0 tests run. No change from 2026-08-04.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 57.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 181 type errors. **STABLE — no change from yesterday (+0).**
- Twelfth consecutive stable day at 181.
- **Action:** Debt from 2026-07-25/26 accumulation (+24) remains live; no new regression today.

## Board-Client
**N/A** — No `messages/board-client` package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 55.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 55.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 55.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-04)
- **NEUTRAL:** Build type errors held at 181 (+0). Twelfth consecutive stable day.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 55.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 57.
- **NO NEW FAILURES** vs yesterday (mcp-server ESM failure noted 2026-08-04 persists but was not re-checked today).
- **Primary blocker remains:** `npm ci` at repo root. Now **57 days unresolved.**
