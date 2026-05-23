# Daily Quality Review — 2026-05-23

> **Path note:** Spec paths `jeff-bridwell-personal-site/` and `messages/{board-client,slack-bridge}/` do not exist in this repo.
> Mapped to actual packages: `directing/clearing` (app), `platform/{workflow-engine,chorus-sdk,pulse,mcp-server,api}`.

## App Tests (`directing/clearing`)
**🔴 RED** — 53 failed / 309 passed / 362 total | 1 suite failing (`clearing-ui.test.ts`)
- **Action:** Triage `clearing-ui.test.ts` failures; Node exit-code 1 on test teardown.

## Lint (`platform/` + `directing/` via root eslint)
**🔴 RED** — 187 problems: 147 errors, 40 warnings
- Dominant issue: `quotes` rule violations (single-quote enforcement)
- Root `@eslint/js` dep was missing; installed during this run to get lint output
- **Action:** Run `npm run lint:fix` at root; commit quote fixes; verify no regressions.

## Build (`platform/api` TypeScript)
**🟢 GREEN** — 0 TypeScript errors

## Board-Client
**⚪ N/A** — `messages/board-client` not found in repo. No equivalent package mapped.

## Workflow-Engine (`platform/workflow-engine`)
**🟢 GREEN** — 3 suites / 62 tests passed / 0 failed

## Chorus-SDK (`platform/chorus-sdk`)
**🟢 GREEN** — 3 suites / 51 tests passed / 0 failed

## Slack-Bridge
**⚪ N/A** — `messages/slack-bridge` not found in repo. No equivalent package mapped.

## Coverage (`directing/clearing`)
| Metric   | % |
|----------|------|
| Stmts    | 86.32 |
| Branch   | 77.65 |
| Funcs    | 88.47 |
| Lines    | 88.41 |

Branch coverage at 77.65% is below the 80% floor — **yellow flag**.

## Failure Delta
No previous `daily-quality-review.md` found — this is the baseline run.

---
*Also checked: `platform/mcp-server` (43/43 pass via tsx), `platform/pulse` (57/57 pass). Both green.*
