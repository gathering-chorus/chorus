# Current Work

Last updated: 2026-08-27 17:50 Boston

## WIP
- **#4022** "parallelize the nightly runner — 10–15m elapsed" (Jeff's bar, set 2026-08-27; filed by Silas, Jeff-initiated). Pulled 17:48, werk `kade/4022`. Shape: run suites concurrently, isolation-declared suites serialized, elapsed on the spine. Design targets from today's live run: `test-chorus-share.sh` 8m, `nudge-single-mcp-path.bats` ~14m/test (full-tree grep, filters after the crawl), `crawler-bespoke-hydrator` wedge. Sibling: #4017 (fold shell suites into typed cases).
  - **Measured 2026-08-27 22:27:** parallel bats pool (4 workers) + per-suite 600s child-exit deadline → **49m12s** full run vs 2.4h serial (3×). OVER BAR alarm fired live (`test.nightly.over_bar`, bar 900s). 0 hangs in 286 units.
  - **To reach 15m:** parallelize cargo lane (one workspace nextest instead of 24 serial crate invocations) + npm lane (bounded jest concurrency); raise the writeback MAX_POSTS=2000 cap (dropped 4,712 of 6,712 — #4015's gate caught it). Worker budget matters: 6 workers pegged load 194; 4 peaked 68 during jest.

## Landed today
- **#4015** (merge `93552e529`, Jeff's go 17:47) — per-test results store end-to-end. The whole chain closed live: join fix (describe-prefixed names), Test-name minting under the door's 128-byte cap (+ no `--`, the door collapses hyphen runs when resolving `ofTest`), RESULTS LOST exits 1 loudly, `/test-run` gains the store-derived "Most recent stored run" section (a run that saved nothing cannot appear on it). Pipeline's own run: 218/218 stored. **Tonight's 03:00 nightly is the first full-scale run under the fixed runner — check the page in the morning.**

## Waiting / handoffs
- Silas re-runs his atlas land after #4015 (cognitive-complexity ratchet unblocked — `auditClose` refactor landed with it). Wren's #3860 queue behind that.
- Cage-escape class folded into **#4005** (mine), bars agreed with Silas: fixtures bring their own world (#3528), fix ships a negative proof (caged run visibly unable to reach a live session). Instances logged as TD-028.

## Open debt — deliberately left red rather than faked green
- 199 security-probe failures in the nightly = authz-coverage program (59 undeclared routes), not new breakage.
- **`coverage:clearing`** — floors unmet in `src/server.ts`; needs real tests, not a lowered floor.
- **`npm:jeff-bridwell-personal-site`** — 83 API endpoints without swagger tags (TD-027); no documentation theater.

## Context
- Report page reads: nightly section from the official log; "Most recent stored run" from the store. Never write a validation run to the shared log (`NIGHTLY_LOG_PATH=<werk-local>` — the July "34 red" lesson).
- Status-loop discipline (today's lesson, in memory): every tick ends visible; two ticks without new output = say STALLED and sample the process, never narrate "probably fine."
