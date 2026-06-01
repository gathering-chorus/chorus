# Daily Quality Review — 2026-06-01

> **Path note:** Spec paths `jeff-bridwell-personal-site/` and `messages/{board-client,slack-bridge}/` do not exist.
> Mapped to: `directing/clearing` (app), `platform/{workflow-engine,chorus-sdk,pulse,mcp-server,api}`.

## App Tests (`directing/clearing`)
**RED** — 0 run / 0 passed / 0 failed (suite blocked)
- `ts-jest` preset not found: node_modules not installed in `directing/clearing`.
- **Action:** Run `npm ci` in `directing/clearing` before test run. Was 53 fail / 309 pass last run.

## Lint (`platform/` + `directing/`)
**YELLOW** — Glob `platform/**/src/**/*.ts` matched 0 files from repo root.
- Actual src dirs are nested deeper (`platform/services/*/src/`, `platform/mcp-server/src/`).
- **Action:** Update lint script glob or run eslint per-package. Status unverifiable this run.

## Build (`platform/api` TypeScript)
**RED** — 419 errors (was 0 on 2026-05-29)
- Root cause: `@types/node` missing — `process`, `path`, `fs`, `fetch`, `Buffer` all unresolved.
- **Action:** Run `npm ci` in `platform/api`. Likely affects all platform packages.

## Board-Client
**N/A** — `messages/board-client` not in repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — 0 run (suite blocked) — `ts-jest` not found. Was 62/62 pass on 2026-05-29.
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — 0 run (suite blocked) — `ts-jest` not found. Was 51/51 pass on 2026-05-29.
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge
**N/A** — `messages/slack-bridge` not in repo. (`platform/pulse` substituted: also blocked — ts-jest missing. Was 57/57 pass.)

## Coverage
**UNAVAILABLE** — No tests could run; coverage not extractable this cycle.

## Failure Delta (vs 2026-05-29)
- **REGRESSION — all packages**: `ts-jest` missing across every TS package (fresh clone, deps not installed).
- **Build**: 0 → 419 errors. `@types/node` not installed in `platform/api`.
- **MCP-Server**: 51/51 pass → 13/13 suites fail (`SyntaxError: import outside module` — Jest config mismatch).
- **Root cause**: `npm ci` not run post-clone. Workspace-level `npm ci` or per-package installs needed.
- **Unblocked last run**: Lint (2688 errors) and 53 app-test failures remain unresolved but untestable this cycle.
