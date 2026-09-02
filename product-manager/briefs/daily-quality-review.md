# Daily Quality Review — 2026-09-02

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse,mcp-server}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 82.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: `@eslint/js` module not found. **Day 84.**
- ESLint cannot load its config; 0 files linted.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**RED** — Per-package error counts (all unchanged from yesterday):
| Package | TS Errors |
|---|---|
| `directing/clearing` | 239 |
| `platform/mcp-server` | 28 |
| `platform/chorus-sdk` | 28 |
| `platform/workflow-engine` | 11 |
| `platform/pulse` | **952** |
- **Action:** Resolve `node_modules` blockage first; `pulse` count (952) is alarming.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 82.**

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 82.**

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 82.**

## MCP-Server (`platform/mcp-server`)
**RED** — 31 suites, 0 tests run. Babel TS syntax errors (separate from ts-jest issue).
- All suites fail: `SyntaxError: Unexpected token` — babel config missing TS plugin.
- **Action:** `npm ci` in `platform/mcp-server`; verify babel preset-typescript is declared.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-09-01)
- **UNCHANGED:** All 4 ts-jest suites blocked — **day 82** (no regression, no progress).
- **UNCHANGED:** Lint blocked — **day 84**.
- **UNCHANGED:** `platform/mcp-server` — 31 suites failing (babel TS).
- **UNCHANGED:** TS error counts identical across all packages.
- **Root blocker (82 days):** `npm ci` at repo root + sub-packages is the single fix needed to unblock all suites. Pulse TS count (952) warrants a dedicated card.
