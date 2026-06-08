# Daily Quality Review — 2026-06-08

> **Path note:** Spec paths `jeff-bridwell-personal-site/` and `messages/{board-client,slack-bridge}/` mapped to
> `directing/clearing` (app) and `platform/{workflow-engine,chorus-sdk,pulse}` respectively.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules installed but ts-jest missing).
- Previous: 309 pass / 53 fail / 362 total (2026-06-06). Now: 0 run.
- **Action:** `npm ci` in `directing/clearing` to restore ts-jest.

## Lint (`directing/clearing`)
**RED** — ESLint blocked: root `eslint.config.js` cannot resolve `@eslint/js` (root node_modules incomplete).
- Persistent since 2026-06-06.
- **Action:** `npm ci` at repo root.

## Build (`directing/clearing` TypeScript)
**RED** — 140 type errors (`npx tsc --noEmit`).
- Previous: 0 errors (2026-06-06). **New regression.**
- **Action:** Investigate — likely related to node_modules state or a recent type-breaking change.

## Board-Client
**N/A** — `messages/board-client` not in repo; no substitute this cycle.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 62/62 pass (2026-06-06). Now: 0 run.
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 52/52 pass, coverage threshold breach on funcs (2026-06-06). Now: 0 run.
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found.
- Previous: 69/69 pass (2026-06-06). Now: 0 run.
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no coverage data collected this cycle.
- Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62.06%), pulse GREEN.

## Failure Delta (vs 2026-06-06)
- **REGRESSION:** All 4 test suites dropped from passing to BLOCKED (ts-jest missing from node_modules).
- **REGRESSION:** Build 0 → 140 TypeScript errors. Root cause unknown — requires investigation.
- **PERSISTENT:** Lint RED (root @eslint/js missing) — unchanged.
- **Root cause hypothesis:** `npm ci` was run somewhere stripping or failing to install devDependencies across packages.
