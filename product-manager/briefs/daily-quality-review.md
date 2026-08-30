# Daily Quality Review — 2026-08-30

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 79.**
- Package-level jest blocked; 0 tests run.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found (root `node_modules` missing). **Day 81.**
- ESLint: "No files matching pattern" — root node_modules absent.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — **239 errors** (+1 from yesterday's 238). **New regression.**
- Crept up by 1 — investigate what changed.
- **Action:** Identify new error introduced today; resolve.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 79.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 79.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 79.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-29)
- **NEW:** Build errors: 238 → **239** (+1). First increase in days — regression.
- **UNCHANGED:** All 4 test suites blocked (`ts-jest preset not found`) — **day 79**.
- **UNCHANGED:** Lint blocked (`@eslint/js` missing) — **day 81**.
- **Root blocker (79 days):** `npm ci` at repo root + all sub-packages.
