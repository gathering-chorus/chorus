# Daily Quality Review — 2026-07-24

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 43.
- 0 tests run. No change from 2026-07-23.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 45.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 157 type errors. **UP +3 from yesterday (was 154). NEW REGRESSION.**
- New errors in `server.ts` lines 132: `_res` and `next` implicitly have `any` type — introduced by kade:#3667 (`#790`, merged 2026-07-23). Remaining delta is `@types/node`/`socket.io` misses (pre-existing).
- **Action:** kade should add explicit Express types to the new middleware params in `server.ts:132`.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 43.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 43.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 43.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-23)
- **NEW REGRESSION:** Build type errors 154 → 157 (+3). Introduced by kade:#3667 — new middleware in `server.ts` uses implicit `any` for `_res`/`next` params.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 43.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 45.
- **Primary blocker remains:** `npm ci` at repo root. Now **45 days unresolved.**
