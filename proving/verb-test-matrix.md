# Verb-Spine Test Matrix — #3324 audit (vc verbs + MCP wrapper)

Audited 2026-06-10 against the contracts live that day (post #3294–#3298 --atomic pass,
#3304 conflict hold/continue, #3311 7-verb surface + finalize-folded-into-accept,
#3317/#3320/#3323 deploy engine — build/deploy rows are #3326, Silas's sibling audit).
Classification rubric: **valid** · **asserts-retired-behavior** · **passes-by-definition** ·
**integration-not-gated**. PATH-shim e2e harnesses (temp repos, shimmed gh/launchctl/codesign/cards)
count as properly gated — they are hermetic by construction.

## Summary

| Suite | Tests | Valid | Retired | By-definition | Un-gated | Action taken (#3324) |
|---|---|---|---|---|---|---|
| werk-pull (units+e2e) | 11 | 11 | 0 | 0 | 0 | — |
| werk-commit (units+e2e) | 21 | 21 | 0 | 0 | 0 | — (freshest; #3304 same-day) |
| werk-push (units+e2e) | 8 | 8 | 0 | 0 | 0 | — |
| werk-merge (units+e2e) | 14 | 14 | 0 | 0 | 0 | — |
| werk-accept (units+e2e) | 12 | 10 | 2 | 0 | 0 | 2 deleted |
| werk-demo (inline+e2e) | 29 | 23 | 5 | 1 | 0 | 6 deleted |
| mcp-server wrapper (7 files) | 24 | 11 | 13 | 0 | 0 | 3 dead files deleted (12 tests); 1 stale assertion flagged |
| **Total** | **119** | **98** | **20** | **1** | **0** | **20 deleted** |

All suites green before AND after deletions. CI: new required `cargo-test-verbs` job runs all
8 werk-* crate suites on every PR (closes the #3229 class).

## Deletions performed (asserts-retired-behavior / passes-by-definition)

- `werk-demo/tests/e2e.rs`: `read_decision_*` ×4 + `decision_exit_codes_match_the_contract` —
  #3279 retired the blocking-decision step; `read_decision`/`Decision::exit_code` have zero
  production callers. (Orphaned lib code removal = fill card, needs #3148 blast-radius.)
- `werk-demo/src/lib.rs`: `announce_skip_drives_tail_for_test_suite` — `true || …`, cannot fail.
- `werk-accept/tests/units.rs`: `demo_decision_line_carries_more_and_no_go` (no-go/more emission
  died with werk-do-more, #3311) + `branch_name_is_role_slash_card` (zero callers since #3175).
- `mcp-server/tests/`: `commit-failure-detail.test.ts`, `acp-already-merged.test.ts`,
  `promote-werk-bin-slot.test.ts` — all three exercised exported helpers with **zero call sites**
  (v1 chorus_commit/chorus_acp/promote paths cut). 12 tests certifying dead code.
- Flagged, not deleted: `chorus-werk-verb.test.ts` "never auto-accepts" asserts the mock's own
  stdout echoing the pre-#3311 "run werk-accept" relay prompt — stale under GO=accept.

## Known-bug confirmed (cockpit "tests: fail")

`werk-demo::demo()` auto-runs `cargo test --lib --bins` at the **werk root** (no Cargo.toml there)
unless `CHORUS_DEMO_SKIP_TEST_RUN=1` → records `demo.test_result=fail`, and latest-wins
**overwrites an explicitly recorded pass** (burned 3 consecutive demos 2026-06-10).
Smallest fix (~4 lines): skip the auto-run when `test_result_recorded()` already has a result;
optionally don't record "fail" when `<werk>/Cargo.toml` is absent (can't-run ≠ ran-and-failed).
→ fill card.

## Gaps (fill-card inputs, by priority)

**P1-shaped (live-incident classes, zero coverage):**
1. **werk-accept: the #3311 fold itself** — `run_accept` (signal+finalize one invocation,
   idempotent re-run completing a partial) never called by any test; the exact path that
   un-orphaned #3320/#3304's half-manual lands today.
2. **MCP wrapper behavior layer** — `executeWerkVerb` env contract unpinned (deleting #3320's
   `CHORUS_INVOKER` stays green = the transport-drop class could silently reopen); `reason=`
   refusal parsing untested against the production parser; 7-verb surface not pinned (a deleted
   #3311 tool reappearing or werk-merge un-registering fails nothing); werk-accept's
   `DEPLOY_ROLE=getCallerRole()` accept-attribution untested.
3. **werk-demo cockpit bug fix + its test** (above) and the #3318 ACT/GITHUB_ACTIONS skip branch
   (team-wide pipeline-break class, no regression guard; the happy-path e2e is env-fragile —
   fails if run under ACT env).

**P2-shaped (refusal-taxonomy holes): CLOSED by #3330 (2026-06-10)**
4. ~~werk-pull~~ — card-not-found / wrong-status / wrong-branch-werk driven + spine'd;
   gh-register-fail rollback asserts the board-status RESTORE (e2e `refusals_are_typed_spined_…`).
5. ~~werk-push~~ — wrong-branch driven + spine'd; gh-register-fail rollback asserts the remote
   ref is deleted (no orphan); `_GIT_QUEUE_PUSH` sentinel proven via a rejecting pre-push hook
   (e2e `wrong_branch_refuses_sentinel_…`).
6. ~~werk-merge~~ — --atomic gate proven to refuse BEFORE side effects (zero gh calls, zero
   witness); merge.approved spine emit captured with {accepter,pr,atomic}; pr-create-fail +
   no-open-pr driven (e2e `atomic_gate_orders_…`).
7. ~~werk-commit~~ — no-werk/wrong-branch driven; commit.completed spine captured with sha;
   continue-RE-conflict (multi-commit replay holds again) proven
   (e2e `refusals_success_spine_and_continue_reconflict`).
8. werk-demo: wrong-status/no-ac refusals driven end-to-end; #3319 prework-standby branch.

**Dead-code removals (need blast-radius, #3148):** werk-demo `read_decision`/`Decision`;
werk-accept `branch_name`; mcp-server orphaned exports (`commitFailureDetail`,
`classifyCommitFailure`, `findMissingPaths`, `prCreateMeansAlreadyMerged`, `resolveWerkBinDir`).

## Proposed fill cards (filed on Jeff's go at demo)

1. **werk-accept fold coverage** — run_accept one-shot + idempotent-partial e2e (gap 1). P2.
2. **MCP wrapper behavior tests** — env contract incl. CHORUS_INVOKER, reason= parse, 7-verb
   surface pin, accept attribution (gap 2). P2.
3. **werk-demo cockpit fix** — explicit test-result survives the auto-run + ACT-skip regression
   guard + e2e env-hardening (gap 3). P2.
4. **Refusal-taxonomy fill, vc verbs** — gaps 4–8 in one card (same fixture patterns). P3.
5. **Dead-code removal pass** — orphans above, with semantic blast-radius. P3.
