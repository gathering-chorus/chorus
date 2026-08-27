# Daily Quality Review — 2026-08-27

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 76.**
- Package-level jest blocked; 0 tests run.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: ESLint `src/` pattern not found at repo root. **Day 78.**
- `src/` exists inside `directing/clearing/` but not at root; lint command targets wrong path.
- **Action:** Fix ESLint target path OR run from `directing/clearing`.

## Build (TypeScript)
**RED** — **946 errors** (up from ~238 yesterday, **+708 new errors**).
- Error trend: 234 → 235 → 234 → 238 → **946**. Massive spike today.
- All new errors: `@types/jest` missing — `describe`/`test`/`expect` not found in test files.
- Root cause likely: `@types/jest` removed from root `node_modules` or tsconfig now includes test files.
- **Action (urgent):** Investigate `@types/jest` removal. Run `npm ls @types/jest` to confirm. This is a new regression as of 2026-08-27.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 76.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 76.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 76.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-26)
- **BUILD: NEW REGRESSION** — TS errors spiked 238 → **946** (+708). All `@types/jest` type errors. Investigate immediately.
- **UNCHANGED:** All 4 package-level test suites blocked (`ts-jest preset not found`) — **day 76**.
- **UNCHANGED:** Lint blocked (`ESLint src/ not found`) — **day 78**.
- **Root blocker (76 days):** `npm ci` at repo root + all sub-packages. Escalation overdue.
