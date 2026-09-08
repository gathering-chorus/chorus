# Daily Quality Review — 2026-09-08

> **Path map:** `directing/clearing` → app; `platform/{mcp-server,workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 87.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: ESLint cannot find `@eslint/js` module. **Day 89.**
- Config at repo root fails to load; no warnings counted.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — Error counts unchanged. No new regressions.
| Package | Yesterday | Today | Delta |
|---|---|---|---|
| `directing/clearing` | 240 | 240 | — |
| `platform/mcp-server` | 250 | 250 | — |
| `platform/workflow-engine` | 11 | 11 | — |
| `platform/chorus-sdk` | 28 | 28 | — |
| `platform/pulse` | 952 | 952 | — |
- **Action:** `platform/mcp-server` spike (+222, landed 2026-09-03) now 5 days old — card needed urgently.

## Board-Client → `platform/mcp-server`
**RED** — **32 suites** (was 31), 0 tests run (Babel TS transform fails). **⚠ +1 new suite.**
- **Action:** Identify new suite added to mcp-server today. Also fix root cause: `babel-plugin-transform-typescript` or switch to ts-jest.

## Workflow-Engine
**RED** — BLOCKED: `ts-jest` preset not found. **Day 87.** Unchanged.

## Chorus-SDK
**RED** — BLOCKED: `ts-jest` preset not found. **Day 87.** Unchanged.

## Slack-Bridge → `platform/pulse`
**RED** — BLOCKED: `ts-jest` preset not found. **Day 87.** Unchanged.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-09-07)
**⚠ 1 new failure.** `platform/mcp-server` grew from 31 → 32 failing suites (a new test file was added to a suite that cannot run).
- All 4 ts-jest suites still blocked (day 87). Fix is `npm ci` per package.
- Lint still blocked (day 89). Fix is `npm ci` at repo root.
- TS error totals flat (1481 across 5 packages). `mcp-server` +222 spike now 5 days stale.
