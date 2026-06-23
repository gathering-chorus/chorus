# Daily Quality Review — 2026-06-23

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 15.
- 0 tests run. **Error type reverted** from npm E404 (2026-06-22) back to ts-jest preset missing.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 17.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 150 type errors. UNCHANGED from 2026-06-22.
- Regression introduced 2026-06-21 (+1 to 150) remains unresolved.
- **Action:** Investigate accumulated type errors.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 15.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 15.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 15.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-06-22)
- **ERROR TYPE REVERTED:** All 4 suites switched back from npm E404 (`browserslist@4.28.4.tgz`) to `ts-jest preset not found`. Suggests node_modules were wiped or an install attempt failed partway through.
- **UNCHANGED:** Build at 150 type errors (regression from 2026-06-21 still unresolved — day 3).
- **UNCHANGED:** Lint blocked (`@eslint/js`) — day 17.
- **Root cause:** node_modules incomplete across all packages; `npm ci` at repo root should unblock all 4 test suites and lint simultaneously.
