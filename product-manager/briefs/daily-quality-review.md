# Daily Quality Review — 2026-07-10

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 29.
- 0 tests run. No change from 2026-07-09.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 31.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 154 type errors. **No change from 2026-07-09** (was 154). Day 3 at this count.
- Regression from 2026-06-21 holding steady; no new errors introduced today.
- **Action:** Investigate 4 type errors introduced 2026-07-02; still unresolved.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 29.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 29.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 29.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-07-09)
- **NO NEW REGRESSIONS:** Build type errors held at 154 (no change). Day 3 unchanged.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 29.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 31.
- **Primary blocker remains:** `npm ci` at repo root unblocks all suites and lint in one step.
