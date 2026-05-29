# Daily Quality Review — 2026-05-29

> **Path note:** Spec paths `jeff-bridwell-personal-site/` and `messages/{board-client,slack-bridge}/` do not exist.
> Mapped to: `directing/clearing` (app), `platform/{workflow-engine,chorus-sdk,pulse,mcp-server,api}`.

## App Tests (`directing/clearing`)
**RED** — 53 failed / 309 passed / 362 total | 1 suite failing (`clearing-ui.test.ts`)
- Root cause: `dist/server.js` not built; tests launch server from hardcoded Mac path.
- **Action:** Build clearing before test run; fix server path to be repo-relative. Persistent from baseline.

## Lint (`platform/` + `directing/` via root eslint)
**RED** — 2730 problems: 2688 errors, 42 warnings (up from 147 errors / 40 warnings on 2026-05-23)
- Dominant issues: `no-undef` for Node globals (`process`, `module`, `fetch`, `Buffer`) across platform files.
- **Action:** Add `node: true` environment to eslint config for platform/* — likely a rule-tightening regression.

## Build (`platform/api` TypeScript)
**GREEN** — 0 TypeScript errors

## Board-Client
**N/A** — `messages/board-client` not in repo.

## Workflow-Engine (`platform/workflow-engine`)
**GREEN** — 3 suites / 62 tests passed / 0 failed

## Chorus-SDK (`platform/chorus-sdk`)
**GREEN** — 3 suites / 51 tests passed / 0 failed

## Slack-Bridge
**N/A** — `messages/slack-bridge` not in repo.

## Coverage (`directing/clearing`)
| Metric | % | vs baseline |
|--------|------|-------------|
| Stmts | 86.32 | = |
| Branch | 77.65 | = |
| Funcs | 88.47 | = |
| Lines | 88.41 | = |

Branch at 77.65% — below 80% floor. No change from baseline.

## Failure Delta (vs 2026-05-23)
- **App Tests:** No change — same 53 failures. Persistent unresolved.
- **Lint:** REGRESSION — errors jumped from 147 → 2688 (+2541). New `no-undef` violations dominate.
- **Build/Platform tests:** No change — all stable.
- *Also checked: `platform/mcp-server` (51/51 pass), `platform/pulse` (57/57 pass). Both green.*
