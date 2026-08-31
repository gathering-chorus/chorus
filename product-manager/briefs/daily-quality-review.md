# Daily Quality Review — 2026-08-31

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 80.**
- Package-level jest blocked; 0 tests run.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found (root `node_modules` missing). **Day 82.**
- ESLint: "Cannot find module @eslint/js" — root node_modules absent.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — **239 errors** (unchanged from yesterday).
- No regression, no improvement.
- **Action:** Resolve ts-jest/node_modules blockage first; re-evaluate.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 80.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 80.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 80.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-30)
- **UNCHANGED:** Build errors: 239 → **239** (stable, no new regression).
- **UNCHANGED:** All 4 test suites blocked (`ts-jest preset not found`) — **day 80**.
- **UNCHANGED:** Lint blocked (`@eslint/js` missing) — **day 82**.
- **Root blocker (80 days):** `npm ci` at repo root + all sub-packages. Nothing new to act on.
