# Daily Quality Review — 2026-07-28

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 47.
- 0 tests run. No change from 2026-07-27.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 49.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 181 type errors. **STABLE — no change from yesterday (+0).**
- Four-day trend: 157 → 174 → 181 → 181 → 181. Held flat for second consecutive day.
- **Action:** Debt from 2026-07-25/26 accumulation (+24) remains live; no regression today.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 47.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 47.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 47.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-27)
- **NEUTRAL:** Build type errors held at 181 (+0). Second consecutive stable day after 157→174→181 regression spike.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now day 47.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 49.
- **Primary blocker remains:** `npm ci` at repo root. Now **49 days unresolved.**
