# Daily Quality Review — 2026-03-31

## App Tests
**YELLOW** — `jeff-bridwell-personal-site` directory not found. Suite cannot run.
Action: Confirm repo path or remove from check matrix. (Persistent — 3rd day.)

## Lint
**YELLOW** — Cannot run; app directory missing. No lint data.

## Build
**YELLOW** — Cannot run; app directory missing. No TypeScript build data.

## Board-Client
**RED** — `ts-jest` not found; `node_modules` absent entirely.
Delta: Unchanged from 2026-03-29. Runner broken.
Action: `cd chorus/platform/board-client && npm install`

## Workflow-Engine
**RED** — `ts-jest` not found; `node_modules` absent entirely.
Delta: Unchanged from 2026-03-29. Runner broken.
Action: `cd chorus/platform/workflow-engine && npm install`

## Chorus-SDK
**RED** — `ts-jest` not found; `node_modules` absent entirely.
Delta: **REGRESSION** — was GREEN on 2026-03-29 (2 suites, 6/6 passed). Now broken.
Action: `cd chorus/platform/chorus-sdk && npm install` — **priority fix.**

## Slack-Bridge
**RED** — `ts-jest` not found; `node_modules` absent entirely.
Delta: Unchanged from 2026-03-29. Runner broken.
Action: `cd chorus/archive/slack-bridge && npm install`

## Coverage
| Package | Status |
|---|---|
| chorus-sdk | N/A — node_modules missing (was 91.7% stmts on 2026-03-29) |
| board-client | N/A — node_modules missing |
| workflow-engine | N/A — node_modules missing |
| slack-bridge | N/A — node_modules missing |

## Failure Delta
**REGRESSION vs. 2026-03-29** — Chorus-SDK dropped from GREEN (6/6) to broken.
Root cause: `node_modules` missing in all 4 platform packages (not just 3).
Fix: `npm install` in board-client, workflow-engine, chorus-sdk, slack-bridge.
Blocker: No test signal on any package for 2+ days. CI coverage is dark.
