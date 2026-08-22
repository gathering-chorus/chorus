# Daily Quality Review — 2026-08-22

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). **Day 72.**
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. **Day 74.**
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 235 type errors. **No change from yesterday (235).**
- Error trend: 217 → 226 → 227 → 230 → 231 → 233 → 235 → **235**. Trend may be leveling.
- **Action:** Monitor for two more days; if trend reverses, investigate recent commits.

## Board-Client
**N/A** — No `messages/board-client` in this repo. (Maps to `platform/` suites.)

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 72.**
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 72.**
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 72.**
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-21)
- **BUILD:** Type errors stable at 235 (0 change). First plateau after seven-day +18 run.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now **day 72**.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now **day 74**.
- **Primary blocker:** `npm ci` at repo root. **74 days unresolved. Escalation overdue.**
