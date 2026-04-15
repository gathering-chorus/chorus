# Daily Quality Review — 2026-03-29

## App Tests
**YELLOW** — `jeff-bridwell-personal-site` directory not found in repo. Suite could not run.
Action: Confirm repo path or remove from check matrix. (Persists from yesterday.)

## Lint
**YELLOW** — Could not run; app directory missing. No lint data available.

## Build
**YELLOW** — Could not run; app directory missing. No TypeScript build data.

## Board-Client
**RED** — Suite failed to launch: `ts-jest` preset not found.
Delta: Yesterday ran (62 failures / 142 passed). Today: runner broken — regression.
Action: **Immediate** — `npm install ts-jest` or restore node_modules in board-client.

## Workflow-Engine
**RED** — Suite failed to launch: `ts-jest` preset not found.
Delta: Yesterday GREEN (61/61). Today: runner broken — regression.
Action: Restore ts-jest in workflow-engine node_modules.

## Chorus-SDK
**GREEN** — 2 suites passed | 6/6 tests passed. No action needed.

## Slack-Bridge
**RED** — Suite failed to launch: `ts-jest` preset not found.
Delta: Yesterday GREEN (60/60). Today: runner broken — regression.
Action: Restore ts-jest in slack-bridge node_modules.

## Coverage
| Package | Stmts | Branch | Funcs | Lines |
|---|---|---|---|---|
| chorus-sdk | 91.7% | 52.9% | 85.7% | 91.3% |
| board-client | N/A (runner broken) | — | — | — |
| workflow-engine | N/A (runner broken) | — | — | — |
| slack-bridge | N/A (runner broken) | — | — | — |

## Failure Delta
**REGRESSION vs. 2026-03-28** — `ts-jest` missing in 3 packages (board-client, workflow-engine, slack-bridge).
- board-client: 62 failures → runner broken (worse)
- workflow-engine: 61 passed → runner broken (regression)
- slack-bridge: 60 passed → runner broken (regression)
Root cause likely: `node_modules` cleared or `ts-jest` dep removed across messages packages.
Action: `cd messages && npm install` or restore ts-jest in each package.
