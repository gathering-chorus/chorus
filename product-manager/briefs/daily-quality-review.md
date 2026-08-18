# Daily Quality Review — 2026-08-18

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 68.
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 70.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 230 type errors (+3 from yesterday's 227). Regression accelerating.
- Error trend: 216 → 217 → 226 → 227 → 230. Three-day increase: +14.
- **Action:** Growing regression; root cause investigation overdue.

## Board-Client
**N/A** — No `messages/board-client` in this repo. (Maps to `platform/` suites.)

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 68.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 68.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 68.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-17)
- **BUILD:** Type errors 227 → 230 (+3). Three consecutive daily increases; +14 over three days.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 68.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 70.
- **Primary blocker:** `npm ci` at repo root. **70 days unresolved. Escalation warranted.**
