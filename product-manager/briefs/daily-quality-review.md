# Daily Quality Review — 2026-09-04

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse,mcp-server}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: ESLint cannot find src files. **Day 85.**
- Pattern `directing/clearing/src/` yields no matches from repo root; module load also fails.
- **Action:** `npm ci` at repo root; run ESLint from within `directing/clearing/`.

## Build (TypeScript)
**RED** — TS error counts today vs yesterday:
| Package | Yesterday | Today | Delta |
|---|---|---|---|
| `directing/clearing` | 239 | 240 | +1 |
| `platform/mcp-server` | 28 | **250** | **+222 ⚠️** |
| `platform/workflow-engine` | 11 | 11 | — |
| `platform/chorus-sdk` | 28 | 28 | — |
| `platform/pulse` | 952 | 952 | — |
- **Action:** mcp-server regression (+222 errors) needs immediate triage — likely a type-definition or import change landed overnight.

## Board-Client → `platform/mcp-server`
**RED** — 31 suites, 0 tests run (ESM/Babel TS transform missing). Unchanged.
- **Action:** Add `babel-plugin-transform-typescript` or switch to ts-jest.

## Workflow-Engine
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.** Unchanged.

## Chorus-SDK
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.** Unchanged.

## Slack-Bridge → `platform/pulse`
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.** Unchanged.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-09-02)
- **NEW REGRESSION:** `platform/mcp-server` TS errors: 28 → 250 (+222). File a card.
- **NEW:** `directing/clearing` TS errors: 239 → 240 (+1). Minor, monitor.
- **UNCHANGED (day 83):** All 4 ts-jest suites blocked — root fix is `npm ci`.
- **UNCHANGED (day 85):** Lint blocked.
- **UNCHANGED:** `platform/pulse` at 952 TS errors.
