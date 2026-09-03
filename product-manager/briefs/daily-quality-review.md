# Daily Quality Review — 2026-09-03

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse,mcp-server}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.**
- 0 tests run. Root cause: missing `node_modules` in package.
- **Action:** `npm ci` in `directing/clearing`.

## Lint
**RED** — BLOCKED: no `src/` at repo root; ESLint finds no files. **Day 85.**
- **Action:** `npm ci` at repo root; verify lint path config.

## Build (TypeScript)
**RED** — TS error counts:
| Package | Yesterday | Today | Delta |
|---|---|---|---|
| `directing/clearing` | 239 | 240 | +1 |
| `platform/mcp-server` | 28 | **250** | **+222 🔴** |
| `platform/chorus-sdk` | 28 | 28 | 0 |
| `platform/workflow-engine` | 11 | 11 | 0 |
| `platform/pulse` | 952 | 952 | 0 |
- **Action:** `platform/mcp-server` TS error spike (+222) is a new regression — needs immediate investigation.

## Board-Client
**N/A** — No `messages/board-client` in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.**

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.**

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 83.**

## MCP-Server (`platform/mcp-server`)
**RED** — 31 suites, 0 tests run. Babel TS syntax errors + 250 TS errors (up from 28 yesterday).
- **Action:** Investigate what changed in `platform/mcp-server` — 222 new TS errors is a regression.

## Coverage
**N/A** — All suites blocked; no data.

## Failure Delta (vs 2026-09-02)
- **NEW REGRESSION 🔴:** `platform/mcp-server` TS errors jumped 28 → 250 (+222). Likely a bad commit or dependency change.
- **NEW REGRESSION (minor):** `directing/clearing` TS errors 239 → 240 (+1).
- **UNCHANGED:** All 4 ts-jest suites blocked — **day 83**.
- **UNCHANGED:** Lint blocked — **day 85**.
- **UNCHANGED:** `platform/mcp-server` — 31 suites failing (babel TS).
- **UNCHANGED:** `platform/pulse` TS count 952; `chorus-sdk` 28; `workflow-engine` 11.
- **Root blocker (83 days):** `npm ci` across repo+packages unblocks all ts-jest suites.
