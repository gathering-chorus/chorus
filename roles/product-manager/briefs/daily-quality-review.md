# Daily Quality Review — 2026-05-12

> Note: Paths `jeff-bridwell-personal-site/`, `messages/board-client/`, `messages/slack-bridge/` do not exist in this repo. Mapped to actual packages; originals marked ABSENT.

---

## App Tests (`platform/api`)
**Status: YELLOW**
Tests: 1393 passed, 1 failed, 11 skipped — 1405 total (142 suites: 141 pass, 1 fail, 1 skip)
Failure: `smoke-pull-card-real.test.ts:46` — `expect.toMatch(/wrong-status|werk-dirty/)` received unexpected value.
Action: Investigate smoke-pull-card-real test; likely needs werk state or card fixture.

## Lint (root workspace)
**Status: RED**
168 problems: 137 errors, 31 warnings (exceeds --max-warnings 10)
Hot spots: `platform/pulse/src/store.ts` (13 quote style), `platform/tests/features/step_definitions/` (2 no-useless-assignment).
Action: Run `npm run lint:fix` to auto-fix quote errors; manually resolve step_defs warnings.

## Build (TypeScript `--noEmit`)
**Status: YELLOW**
`platform/workflow-engine`, `platform/chorus-sdk`, `platform/api`, `platform/pulse`, `directing/clearing`: 0 errors.
`directing/products/cards`: 1 error (`Cannot find module 'chorus-sdk'` in `src/events.ts`).
Action: Add `chorus-sdk` to cards' tsconfig paths or install as dep.

## Board-Client (`messages/board-client`)
**Status: ABSENT** — path not found. `platform/pulse` (likely equivalent): 3 suites, 57 passed, 0 failed. GREEN.

## Workflow-Engine (`platform/workflow-engine`)
**Status: GREEN**
3 suites, 62 tests — all passed. No failures.

## Chorus-SDK (`platform/chorus-sdk`)
**Status: GREEN (tests) / YELLOW (coverage)**
3 suites, 45 tests — all passed.
Coverage: Stmts 76.85% (floor 80% ↓), Branch 80%, Funcs 59.25% (floor 75% ↓), Lines 81.05%.
Action: Coverage below threshold in 2 metrics; address function coverage gaps.

## Slack-Bridge (`messages/slack-bridge`)
**Status: ABSENT** — path not found. No equivalent package identified.

## Coverage
| Package | Stmts | Branch | Funcs | Lines | vs Floor |
|---|---|---|---|---|---|
| workflow-engine | 93.45% | 87.5% | 96.77% | 97.85% | ✓ all above |
| chorus-sdk | 76.85% | 80% | 59.25% | 81.05% | ✗ stmts, funcs |
| pulse | 96.27% | 90.52% | 89.79% | 98.95% | ✓ all above |
| clearing | not collected (suite failure) | | | | |
| cards | not collected (suite failure) | | | | |

## Failure Delta
**First run — no previous baseline.** New issues to track:
- 53 failing tests in `directing/clearing` (clearing-ui.test.ts — MODULE_NOT_FOUND, server won't start)
- 24 failing suites in `directing/products/cards` (chorus-sdk + workflow-engine dist not linked)
- 1 failing test in `platform/api` (smoke-pull-card-real)
- Lint: 137 errors across workspace
