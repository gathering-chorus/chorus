# Daily Quality Review — 2026-06-06

> **Path note:** Spec paths `jeff-bridwell-personal-site/` and `messages/{board-client,slack-bridge}/` do not exist.
> Mapped to: `directing/clearing` (app), `platform/{workflow-engine,chorus-sdk,pulse}` (message packages).

## App Tests (`directing/clearing`)
**YELLOW** — 309 pass / 53 fail / 362 total (1 suite blocked)
- Blocked suite: `tests/clearing-ui.test.ts` — missing `dist/server.js` (build artifact not present in this env).
- All other 12 suites pass. Fail count unchanged from 2026-06-01.
- **Action:** Run `npm run build` in `directing/clearing` before test run to unblock UI suite.

## Lint (`directing/clearing`)
**RED** — ESLint cannot run: root `eslint.config.js` requires `@eslint/js` but root `node_modules/` not installed.
- **Action:** Run `npm ci` at repo root to restore root-level eslint deps.

## Build (`platform/api` TypeScript)
**GREEN** — 0 errors (`npx tsc --noEmit` clean after `npm ci` in `platform/api`). Recovered from 419 errors on 2026-06-01.

## Board-Client
**N/A** — `messages/board-client` not in repo. No substitute identified this cycle.

## Workflow-Engine (`platform/workflow-engine`)
**GREEN** — 62/62 pass. Recovered from blocked (0 run) on 2026-06-01.

## Chorus-SDK (`platform/chorus-sdk`)
**YELLOW** — 52/52 pass. Coverage threshold breach: functions 62.06% vs 75% floor.
- **Action:** Add function-level tests; threshold set in jest config. Was 51/51 on 2026-05-29.

## Slack-Bridge → Pulse (`platform/pulse`)
**GREEN** — 69/69 pass. Recovered from blocked on 2026-06-01.

## Coverage
| Package          | Stmts  | Branch | Funcs  | Lines  | Status  |
|------------------|--------|--------|--------|--------|----------|
| clearing         | 86.32% | 77.65% | 88.47% | 88.41% | YELLOW (server.ts stmts 77.94% < 80%) |
| workflow-engine  | 93.45% | 87.50% | 96.77% | 97.85% | GREEN   |
| chorus-sdk       | 81.06% | 82.01% | 62.06% | 84.21% | RED (funcs 62.06% < 75%) |
| pulse            | 90.27% | 81.25% | 84.21% | 92.27% | GREEN   |

## Failure Delta (vs 2026-06-01)
- **RECOVERED:** workflow-engine 0→62 pass, chorus-sdk 0→52 pass, pulse 0→69 pass (deps installed).
- **RECOVERED:** platform/api build 419→0 errors.
- **UNCHANGED:** clearing 53 failures (UI suite, missing build artifact) — same root as prior runs.
- **NEW:** chorus-sdk functions coverage threshold breach (62.06% < 75%).
- **PERSISTENT:** Lint blocked (root node_modules not installed); clearing server.ts stmts threshold miss.
