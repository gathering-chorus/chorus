# Daily Quality Review — 2026-08-19

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 69.
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 71.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 231 type errors (+1 from yesterday's 230). Slow bleed continues.
- Error trend: 217 → 226 → 227 → 230 → 231. Five-day increase: +14.
- **Action:** Growing regression; root cause investigation overdue.

## Board-Client
**N/A** — No `messages/board-client` in this repo. (Maps to `platform/` suites.)

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 69.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 69.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 69.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-18)
- **BUILD:** Type errors 230 → 231 (+1). Continues slow daily bleed; +14 over five days.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 69.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 71.
- **Primary blocker:** `npm ci` at repo root. **71 days unresolved. Escalation warranted.**
