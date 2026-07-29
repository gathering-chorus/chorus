# Daily Quality Review — 2026-07-29

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 48.
- 0 tests run. No change from 2026-07-28.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 50.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 181 type errors. **STABLE — no change from yesterday (+0).**
- Five-day trend: 157 → 174 → 181 → 181 → 181 → 181. Three consecutive stable days.
- **Action:** Debt from 2026-07-25/26 accumulation (+24) remains live; no regression today.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 48.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 48.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 48.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-28)
- **NEUTRAL:** Build type errors held at 181 (+0). Third consecutive stable day.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 48.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 50.
- **Primary blocker remains:** `npm ci` at repo root. Now **50 days unresolved.**
