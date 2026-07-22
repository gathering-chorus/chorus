# Current Work

Last updated: 2026-07-22 11:05 Boston

## WIP
- **#3663** cards SDK leaked timeout + nightly coverage red — built, committed (`201ff887`), pipeline running to demo stop.
  - Fix 1 (the carded diagnosis): `clearTimeout` in `finally` for `sendCardApprovalNudge`'s abort timer — the rejection path stranded a 5s timer, jest force-exited rc≠0.
  - Fix 2 (found during AC3, baseline-proven pre-existing): `cards-api-permutation.test.ts` never mocked `../src/blast-radius` — move→WIP rows fetched live :3000/:3340 (5s cap per call) and blew the jest budget under coverage instrumentation. Mocked at the module seam like every sibling moveCard suite. Without this, AC4 stays red regardless of the timer fix.
  - New tests: 3 fake-timer settle-path tests (getTimerCount=0), 1 hermetic move-WIP guard (`move-wip-hermetic.test.ts` — blast-radius mocked, asserts nothing else fetches).
  - Proofs: `--detectOpenHandles` rc=0 529/529 zero handles; `--coverage` rc=0, 81.22% stmts ≥ floor 80; lint touched files rc=0.
  - AC4 = tomorrow's nightly grades coverage:cards with a real pct.

## Waiting
- **#3662** nightly wedge guard — landed pre-crash (fbb4db332), guard harness-proven on canonical, LaunchAgent loaded. Tonight's 03:00 run is the first live proof; read the log tomorrow (see next-session.md).

## Context
- Library mac OOM'd hard ~08:54 7/22 (2nd in 2 weeks) — Silas owns follow-up. 7/7 action items (swap alert 8GB + subagent concurrency budget) never carded.
- 14 red suites in the 7/22 nightly — full list in next-session.md. Jeff scoped me to #3662/#3663; the red-list burn-down is unowned on the board.
- Jeff flagged the tests chunk (#1778/#1779) as incomplete — the specific gap: existing nightly reds have no owning cards; those two are new-coverage cards.
