# Current Work

Last updated: 2026-09-09 18:05 Boston

## WIP
- **#4131 Nightly to zero, round two** — werk kade-4131. AC is Jeff's sentence (17:36): every unit ran and passed on /nightly, two mornings in a row; zero red, zero skip, zero UNMEASURED, zero self-refused. No go ask until every box is ticked (Jeff 12:04, 17:38). Worklist 1 (the 12:12 run's 10 reds) done in commits f6e3709/9491ffa/d76ac34. Worklist 2 (the 7 units with no result) done uncommitted 18:00: drift audits index the 2 GB spine once (22s/40s, were dying at cap); membrane drill = freshness verdict under the nightly with an attended-run ledger; shell summary parser prefers the harness line; UNMEASURED rows persist their output; smoke probe 3x; restore-drill = freshness verdict; shared/ not a crate. Proofs: platform/tests/4131-nightly-no-skips.bats (12) + lib.rs shell_counts_4131 (3).
- Known reds to come, on this card: restore-drill last proven 2026-08-31, RED at 10d = 09-11 03:00 unless the Sunday weekly agent passes (it left no verdict 09-06). Membrane has NO attended run on this box = RED until someone runs `MEMBRANE_ALLOW_UNDER_AGENT=1 platform/scripts/test-product-membrane.sh` from an ops shell (stops every chorus service ~2 min).
- After land: the new bats file must be in the tests registry before 03:00 (tag-tests-domain.py is the Test hydrator; check whether the crawler hydrate runs it before the nightly, else run it from canonical after merge).

## Next (in order)
1. **Pipelines card (at Jeff's bouncer, awaiting approve)** — agreed shape locked 15:48: instances = the TWO real pipelines, werk (commit→build→test→demo→land) and athena (shape→forge→seed→validate); clearing + borg modeled with status=planned, no invented steps; runs AND steps emit metrics (Jeff 15:46); Pipeline/PipelineRun shapes + claims mount the API. Split: me TTL, Wren claims/loom review, Silas gates shapes.
1. Silas #4029 deploys the 7 restored domains (my carrier file is on main; MODEL_SET add is his). When /domains serves `builds` again: rebuild the test registry — `python3 platform/scripts/tag-tests-domain.py` (whole jest names now, +151) — then one run, then the red list by owner. Jeff's ask 08-29 14:04: "register the tests and get a report w no red."
2. Report page: split "unregistered" (no identity to save under) from "lost" — needs the runner's unmatched count in the log; today it still says LOST for the 594.
3. Export tool: live-only model → source (the pen has no write-back). #3982 showed the other failure: source dropped while live kept serving; a "served but not in source" check is the same tool.
4. `memory_steps.ts:9` hardcodes prod :3340 (nightly cucumber hits prod's crawler, 22% CPU).
5. Coverage in a werk: cards needs platform/workflow-engine/dist built; werk-build does not build it.

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
