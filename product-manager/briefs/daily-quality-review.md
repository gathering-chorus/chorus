# Daily Quality Review — 2026-07-09

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 28.
- 0 tests run. No change from 2026-07-08.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 30.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 154 type errors. **No change from 2026-07-08** (was 154). Day 2 at this count.
- Regression from 2026-06-21 holding steady; no new errors introduced today.
- **Action:** Investigate 4 type errors introduced 2026-07-02; still unresolved.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 28.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 28.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 28.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-07-08)
- **NO NEW REGRESSIONS:** Build type errors held at 154 (no change).
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 28.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 30.
- **Primary blocker remains:** `npm ci` at repo root unblocks all suites and lint in one step.
