# Daily Quality Review — 2026-09-06

> **Path map:** `directing/clearing` → app; `platform/{mcp-server,workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 85.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: ESLint cannot find `@eslint/js` module. **Day 87.**
- Config at repo root fails to load; no warnings counted.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — Error counts unchanged from yesterday. No new regressions.
| Package | Yesterday | Today | Delta |
|---|---|---|---|
| `directing/clearing` | 240 | 240 | — |
| `platform/mcp-server` | 250 | 250 | — |
| `platform/workflow-engine` | 11 | 11 | — |
| `platform/chorus-sdk` | 28 | 28 | — |
| `platform/pulse` | 952 | 952 | — |
- **Action:** `platform/mcp-server` (+222 spike landed 2026-09-03) still needs a card; `platform/pulse` at 952 is the long tail.

## Board-Client → `platform/mcp-server`
**RED** — 31 suites, 0 tests run (Babel TS transform fails). Unchanged.
- **Action:** Add `babel-plugin-transform-typescript` or switch to ts-jest.

## Workflow-Engine
**RED** — BLOCKED: `ts-jest` preset not found. **Day 85.** Unchanged.

## Chorus-SDK
**RED** — BLOCKED: `ts-jest` preset not found. **Day 85.** Unchanged.

## Slack-Bridge → `platform/pulse`
**RED** — BLOCKED: `ts-jest` preset not found. **Day 85.** Unchanged.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-09-05)
**No new failures.** All counts stable.
- All 4 ts-jest suites still blocked (day 85). Fix is `npm ci` per package.
- Lint still blocked (day 87). Fix is `npm ci` at repo root.
- All TS error counts flat — the `mcp-server` spike (+222) is now 3 days old; needs a card if not already filed.
