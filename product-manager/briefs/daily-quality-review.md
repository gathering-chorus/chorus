# Daily Quality Review — 2026-07-27

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 46.
- 0 tests run. No change from 2026-07-26.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 48.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 181 type errors. **STABLE — no change from yesterday. Regression trend paused.**
- Three-day trend: 157 → 174 → 181 → 181. Today held flat (+0). Yesterday's +7 did not continue.
- **Action:** Still needs resolution; two-day accumulation (+24 over 2026-07-25/26) remains live debt. Monitor for tomorrow.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 46.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 46.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 46.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-26)
- **NEUTRAL:** Build type errors held at 181 (+0 today). Three-day regression trend (157→174→181) paused — no new debt.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 46.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 48.
- **Primary blocker remains:** `npm ci` at repo root. Now **48 days unresolved.**
