# Daily Quality Review — 2026-08-29

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 78.**
- Package-level jest blocked; 0 tests run.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found (root `node_modules` missing). **Day 80.**
- ESLint errors on both root and `directing/clearing` invocation paths.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**YELLOW** — **238 errors** (unchanged from yesterday).
- Stable at baseline; no spike recurrence.
- **Action:** Resolve underlying type errors; 238 is unacceptably high.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 78.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 78.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 78.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-28)
- **UNCHANGED:** All 4 package-level test suites blocked (`ts-jest preset not found`) — **day 78**.
- **UNCHANGED:** Lint blocked (`@eslint/js` missing) — **day 80**.
- **UNCHANGED:** Build at 238 TS errors — no change.
- **Root blocker (78 days):** `npm ci` at repo root + all sub-packages. Zero new failures; zero recovery.
