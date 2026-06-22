# Daily Quality Review — 2026-06-22

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: npm registry E404 for `browserslist@4.28.4.tgz`. Day 14.
- 0 tests run. **Error type changed** from ts-jest preset (node_modules empty) to npm registry 404.
- **Action:** `browserslist@4.28.4` may be yanked. Pin to a valid version or run `npm install` with updated lock.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 16.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root (blocked by registry 404 above — same root cause).

## Build (`directing/clearing` TypeScript)
**RED** — 150 type errors. UNCHANGED from 2026-06-21.
- Yesterday's +1 regression (149 → 150) is the current baseline. No new regressions today.
- **Action:** Investigate 150 accumulated type errors; 1 was introduced 2026-06-21.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: npm registry E404 for `browserslist@4.28.4.tgz`. Day 14.
- **Action:** Same root cause as App Tests — resolve browserslist version.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: npm registry E404 for `browserslist@4.28.4.tgz`. Day 14.
- **Action:** Same root cause as App Tests.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: npm registry E404 for `browserslist@4.28.4.tgz`. Day 14.
- **Action:** Same root cause as App Tests.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-06-21)
- **NEW BLOCKER TYPE:** All 4 test suites now fail with npm E404 (`browserslist@4.28.4.tgz` not in registry), replacing the ts-jest preset error. Suggests an install attempt ran but hit a yanked package.
- **UNCHANGED:** Build at 150 type errors (regression from yesterday still unresolved).
- **UNCHANGED:** Lint blocked (`@eslint/js`) — day 16.
- **Root cause shift:** `browserslist@4.28.4` likely yanked from registry. Update lock file to pin to latest stable browserslist to unblock all 4 suites at once.
