# Current Work

Last updated: 2026-09-03 17:35 Boston

## WIP
- **#4008 single-flight guards the runner** — werk kade-4008. The steal path now asks whether any nightly RUNNER is alive (ps marker `werk-test --nightly|nightly-suites.sh --run-all`, seam NIGHTLY_PS) before stealing a dead holder's lock; refusal is typed and names the runner pid + age, ops-nudge fired, exit 0; `--lock-probe` prints ACQUIRED / REFUSED <reason> for the proof. AC1 (process group + trap reaps the lane) was already landed by #4009/#4035 and is covered by their proofs. Bats 4008: 6 cases incl. both states. Also in this werk: commitment-card-stubs.ttl uses chorus:label so the /sup walk stops showing "card-4008".
- Landed today: #4063 (tests-domain 289→149 + api watermarks crash), #4089 (tests design as 35 Commitment rows; Commitment under services; harvester keeps co-tenants; seed --deploy per-home graphs), #4093 (service page engineering half: flows, runs-as, domain facets via one renderer; registry-first Tests facet).
- Owed, in flow next (tests chunk): the census page-fetch timeout (page 2 of /testresults at 100k rows exceeds 120s under load → row reads UNMEASURED and the readout calls that "fixed"), coverage output kept on fail, 112 registered files no lane runs, TestResult 422s (144 in run 196).
- **#4040 Pipelines modeled** — werk chorus-werk/kade-4040, branch kade/4040. DONE in werk: pipelines-4040.ttl (TBox: PipelineStep/hasStep/executor/forPipeline + run metrics; 3 shapes, instancesGraph=urn:chorus:instances) in MODEL_SET; pipeline-instances.ttl (cicd 5 steps w/ demo=human, athena 4 steps; clearing/borg planned, no steps) in instance-seed-manifest; PipelineRun claimed on the pipelines domain; nightly-suites.sh emits one PipelineRun row (POST /pipelineruns: outcome/duration/testsRun-Failed-Stored, forPipeline=pipeline-cicd). Suite 4040-pipelines-model.bats: 13/15 green hermetic; 2 red = live-serve, green at deploy. AC1 unblocked (Wren 16:22: Document claimed under knowledge; pipelines-design-seed.sh mints Document+Service post-deploy, commit 35f4e00). REMAINING: /cw pipeline (build->deploy->demo) once the 16:06 full run frees the box (monitor armed); Wren claims+loom review; Silas /gate-arch on shapes. NOTE: bats heredoc-in-@test defeats bats failure detection (proven here); no heredocs in .bats.
- #4035 landed + accepted 2026-08-31 14:47 (merge f42a927aa). **#4035 landed + accepted 2026-08-31 14:47** (merge f42a927aa, Jeff's go at the 14:45 demo): unit cap never inherits the wrapper's lane cap (a stuck suite dies at 20 min, not 2h); a stopped run writes RUN|stopped, /nightly says STOPPED, the runner's process group dies with the stop; sdk.ts lazy engine require ended the recurring 3 triggerWorkflow nightly reds.
- **Daily run fired 14:49** under the landed fix (Jeff's ask: a daily test run). Report shape agreed: one line — ran / on time / N failed. Watching it; result goes to Jeff when it completes.

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
