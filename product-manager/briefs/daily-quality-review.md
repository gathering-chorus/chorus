# Daily Quality Review — 2026-08-17

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 67.
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 69.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 227 type errors (+1 from yesterday's 226). Regression continuing but slowed.
- Error trend: 216 → 217 → 226 → 227. Yesterday's spike (+9) has not recurred.
- Known files: `src/transcript.ts:171,173` (`string | null` → `string`); `src/word-cap.ts:115` (`process` undefined).
- **Action:** Still needs investigation; +1 today is better than +9 yesterday but trend is still upward.

## Board-Client
**N/A** — No `messages/board-client` package in this repo. (See `platform/` suites below.)

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 67.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 67.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 67.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-16)
- **BUILD:** Type errors 226 → 227 (+1). Yesterday's +9 spike did not repeat; trend still upward.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 67.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 69.
- **Primary blocker:** `npm ci` at repo root. **69 days unresolved.**
