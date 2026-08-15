# Daily Quality Review — 2026-08-15

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 65.
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 67.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 217 type errors (+1 from yesterday's 216). REGRESSION CONTINUES.
- Same 3 specific errors: `src/transcript.ts:171,173` (`string | null` → `string`); `src/word-cap.ts:115` (`process` undefined, needs `@types/node`).
- **Action:** Fix nullable string handling in `transcript.ts:171,173`; add `@types/node` for `word-cap.ts:115`.

## Board-Client
**N/A** — No `messages/board-client` package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 65.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 65.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 65.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-14)
- **NEW:** Build type errors grew 216 → 217 (+1). No new error files, error count creeping up.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 65.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 67.
- **Primary blocker:** `npm ci` at repo root. **67 days unresolved.**
