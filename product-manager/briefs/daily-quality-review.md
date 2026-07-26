# Daily Quality Review — 2026-07-26

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 45.
- 0 tests run. No change from 2026-07-25.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 47.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 181 type errors. **UP +7 from yesterday (was 174). SECOND CONSECUTIVE REGRESSION DAY.**
- Two-day trend: 157 → 174 → 181 (+24 total over 2 days). Source analysis needed — yesterday's +17 attributed to wren:#3679 + silas:#3669. Today's +7 requires investigation.
- **Action:** Identify commits since 2026-07-25 introducing new type errors. Priority: wren + silas audit route handler types.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 45.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 45.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 45.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-25)
- **NEW REGRESSION:** Build type errors 174 → 181 (+7 today, +24 over 2 days). Second consecutive day of regression. Two-day trend: 157 → 174 → 181. Requires immediate investigation of today's commits.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 45.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 47.
- **Primary blocker remains:** `npm ci` at repo root. Now **47 days unresolved.**
