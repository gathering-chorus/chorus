# Daily Quality Review — 2026-08-21

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 71.
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 73.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 235 type errors (+2 from yesterday's 233). Trend accelerating.
- Error trend: 217 → 226 → 227 → 230 → 231 → 233 → **235**. Seven-day increase: +18.
- **Action:** Rate is +2/day for two consecutive days. Immediate investigation needed.

## Board-Client
**N/A** — No `messages/board-client` in this repo. (Maps to `platform/` suites.)

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 71.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 71.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 71.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-20)
- **BUILD:** Type errors 233 → 235 (+2). Second consecutive day at +2/day. Seven-day total: +18.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 71.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 73.
- **Primary blocker:** `npm ci` at repo root. **73 days unresolved. Escalation overdue.**
