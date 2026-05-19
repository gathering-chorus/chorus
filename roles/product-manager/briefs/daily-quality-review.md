# Daily Quality Review — 2026-05-19

> Paths `jeff-bridwell-personal-site/`, `messages/board-client/`, `messages/slack-bridge/` absent; mapped to real packages.

---

## App Tests (`platform/api`)
**Status: RED** ▲ REGRESSED
Tests: 1424 passed, 7 failed, 11 skipped — 1442 total (144 of 145 suites: 141 pass, 3 fail, 1 skip)
New failures (6): `athena-tree.test.ts` — Zod schema validation; tree.json fails parse+validates, domain/product/service integrity checks.
Persistent failure (1): `smoke-pull-card-real.test.ts:46` — received `card-not-found` instead of `wrong-status|werk-dirty`.
Also: `mcp-nudge-composition.test.ts` — suite-level failure (spine write error: `~/.chorus/chorus.log` ENOENT).
Action: Fix `data/athena/tree.json` schema compliance; create `~/.chorus/chorus.log` or stub for CI; triage smoke fixture.

## Lint (root workspace)
**Status: RED** ▲ WORSE (+56 errors, +12 warnings vs 2026-05-12)
236 problems: 193 errors, 43 warnings. Hot spots: `platform/pulse/src/store.ts` (quote style), `platform/tests/features/step_definitions/` (no-useless-assignment), unused eslint-disable directives.
Action: `npm run lint:fix` covers quote errors; manually resolve unused-disable and assignment warnings.

## Build (TypeScript `--noEmit`, `platform/api`)
**Status: GREEN**
0 type errors. No regression from last week.

## Board-Client (`messages/board-client` → `platform/mcp-server`)
**Status: GREEN**
Original path absent. `platform/mcp-server`: 9 tests, 9 passed, 0 failed (tsx runner).

## Workflow-Engine (`platform/workflow-engine`)
**Status: GREEN**
3 suites, 62 tests — all passed. No change from 2026-05-12.

## Chorus-SDK (`platform/chorus-sdk`)
**Status: GREEN**
3 suites, 45 tests — all passed. No change from 2026-05-12.

## Slack-Bridge (`messages/slack-bridge`)
**Status: ABSENT** — no equivalent package identified in this repo.

## Coverage (`platform/api` — all files)
| Metric | Today | Prior |
|---|---|---|
| Stmts | 77.22% | not collected |
| Branch | 66.77% | not collected |
| Funcs | 73.55% | not collected |
| Lines | 79.07% | not collected |
Branch coverage (66.77%) is the weakest metric; funcs (73.55%) below prior chorus-sdk floor of 75%.

## Failure Delta (vs 2026-05-12)
- **REGRESSED**: api tests +6 new failures (athena-tree Zod schema suite — new since last week)
- **REGRESSED**: Lint +56 errors, +12 warnings (236 total vs 168)
- **STABLE**: workflow-engine (62/62), chorus-sdk (45/45)
- **RESOLVED**: `directing/clearing` and `directing/products/cards` failures not reproduced (not run today)
- **NEW**: mcp-server confirmed green (9/9); not previously tracked
Action needed: athena-tree regression is the highest-priority new failure — likely a schema change landed without updating the fixture.
