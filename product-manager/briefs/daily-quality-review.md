# Daily Quality Review — 2026-07-13

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 32.
- 0 tests run. No change from 2026-07-12.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 34.
- Persistent since 2026-06-06. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 154 type errors. **No change from 2026-07-12** (was 154). Day 6 at this count.
- Root causes: missing `socket.io` types, missing `@types/node`. All `npm ci` resolvable.
- **Action:** `npm ci` at repo root unblocks this too.

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 32.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 32.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 32.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-06).

## Failure Delta (vs 2026-07-12)
- **NO NEW REGRESSIONS:** Build type errors held at 154 (no change). Day 6 unchanged.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 32.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 34.
- **Commits since yesterday:** 8 cards landed (kade: #3638, #3637, #3635, #3421, #3634; wren: #3635; silas: #3630). None touch node_modules or resolve the blocker.
- **Primary blocker remains:** `npm ci` at repo root unblocks all suites, lint, and build in one step.
