# Daily Quality Review — 2026-09-01

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse,mcp-server}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 81.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found. **Day 83.**
- ESLint cannot load its config; 0 files linted.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — Per-package error counts (all unchanged from yesterday):
| Package | TS Errors |
|---|---|
| `directing/clearing` | 239 |
| `platform/mcp-server` | 250 |
| `platform/chorus-sdk` | 28 |
| `platform/workflow-engine` | 11 |
| `platform/pulse` | **952** |
- **Action:** Resolve `node_modules` blockage first; `pulse` count (952) is alarming.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 81.**

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 81.**

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 81.**

## MCP-Server (`platform/mcp-server`) ⚠️ NEW VISIBILITY
**RED** — 31 suites, 0 tests run. Babel TS syntax errors (separate from ts-jest issue).
- All suites fail: `SyntaxError: Unexpected token` — babel config missing TS plugin.
- **Action:** `npm ci` in `platform/mcp-server`; verify babel preset-typescript is declared.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-08-31)
- **UNCHANGED:** `directing/clearing` build errors: 239 → **239**.
- **UNCHANGED:** 4 ts-jest suites blocked — **day 81**.
- **UNCHANGED:** Lint blocked — **day 83**.
- **NEW:** `platform/mcp-server` — 31 suites now visible, all failing (babel TS, not ts-jest).
- **NEW DATA:** `platform/pulse` TypeScript errors: **952** (first time measured; concerning).
- **Root blocker (81 days):** `npm ci` at repo root + sub-packages. Pulse TS count warrants a separate look.
