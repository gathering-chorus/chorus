# Daily Quality Review — 2026-08-28

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 77.**
- Package-level jest blocked; 0 tests run.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found (root `node_modules` missing). **Day 79.**
- ESLint errors on both root and `directing/clearing` invocation paths.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**YELLOW** — **238 errors** (was 946 yesterday, **down 708** — `@types/jest` spike resolved).
- Back to baseline level (234–238 range seen prior to yesterday's spike).
- **Action:** Track root cause of yesterday's spike; 238 errors still needs resolution.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 77.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 77.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 77.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-27)
- **BUILD: RECOVERED** — TS errors dropped 946 → **238** (-708). Yesterday's `@types/jest` spike is gone; back to prior baseline.
- **UNCHANGED:** All 4 package-level test suites blocked (`ts-jest preset not found`) — **day 77**.
- **UNCHANGED:** Lint blocked (`@eslint/js` missing) — **day 79**.
- **Root blocker (77 days):** `npm ci` at repo root + all sub-packages. No new failures today.
