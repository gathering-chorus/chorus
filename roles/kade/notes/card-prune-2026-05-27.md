# Card Prune Working Notes — 2026-05-27

Kade-owned card reduction exercise with Jeff. Target: 107 → 40-50. Walk-through pattern (no batch scans). Notes accumulated as we go.

---

## 1. Disposition log

One line per card: `#NNNN — title fragment — disposition — reason`

- #2441 — Quality review polish wave-2-4 — won't-do — wave-1 enough; upstream substrate makes polish lower-priority by design
- #2434 — Quality scanner alias-aware fix — won't-do — lipstick-on-a-pig; AC unreachable from current scanner substrate (no alias model to ground against)
- #2158 — Pulse SLO audit — reassigned to Wren — belongs with pulse subproduct owner; sidecar #2442 likely subsumes anyway

## 2. Pattern observations (emergent — fill as evidence accumulates)

Things to watch for as patterns:

- **Lipstick-on-a-pig**: point-fix cards on broken substrate; AC can be written but can't be met without re-architecting beneath
- **Void-by-substrate**: AC unreachable from current substrate without 1-2 layers of foundation work first (sharper class than lipstick)
- **Wrong-owner**: cards filed against Kade that actually belong to another role's subproduct
- **Stale-by-time**: filed > N weeks ago, zero comment activity, world has moved
- **Subsumed-by-other-work**: live work elsewhere makes this moot
- **Sequence-retired**: under a sub-product or sequence that's been downgraded/closed
- **Self-contradicting-card**: title says one thing, body says another (the #2158 "1-3ms vs 548ms" drift)

Initial readings (3 cards in):
- Quality-scanner / pipeline / reporter cluster — multiple cards point at the same broken substrate (scanner→SQLite→Fuseki). Strong lipstick concentration.
- Wren's April "Pull soon" comments — cards she flagged for me that I never pulled in 5+ weeks. Pattern: Kade in operator mode while Wren was naming priorities.

## 3. Substrate cards implied (un-filed work surfaced by closures)

When a card closes "void-by-substrate," note the substrate work it implies.

- **Find→Fuseki data pipeline** (implied by #2434, #2441; collapses the scanner→SQLite→Fuseki accretion)
- **Code/test/subdomain/alias as first-class graph instances** (implied by #2434; makes alias-aware queries possible by construction)

## 4. Reassignment patterns

Cards reassigned away — what shape, what role, why misfiled originally.

- #2158 → Wren (pulse subproduct; was Kade because pulse touches chorus-hooks but the substrate-work belongs to pulse-owner)

## 5. Subproduct / sequence concentration (where do my cards cluster?)

Track which subproducts have the heaviest Kade-card load. Reveals where substrate work most likely lives.

(Fill in as we go.)

---

## Process notes

- Walking through deliberately at Jeff's pace, no batch scans (per lesson from this morning re: pattern-match-at-scale)
- Each card gets: read, brief surface to Jeff, his call, action + comment with reason
- Observations accumulated here, distilled at natural stopping points into memory entries and possibly a substrate-card-seed

## Walk-through additions

- #2753 — chorus_acp --delete-branch bug — won't-do — superseded by werk-accept v2 (L248 calls gh pr merge WITHOUT --delete-branch); the bug pattern cannot occur on v2 path by construction
- #2781 — Clean canonical + daemon + settings drift — won't-do — done by other werk-v2 work (#2913 ephemeral, settings env, repair subcommand). 5 of 6 AC verified live; remaining commit-health flags are tool-checking-obsolete-model

### New category: "Already-done-not-closed / superseded-by-newer-code"
- Cards where substantive work happened via adjacent card or substrate change; AC met by construction but card never moved to Done
- Most common via werk-v2 substrate work (verb-binary chain retires chorus_acp's bug surface)
- Verify per card before closing — pattern-match would be a failure

### New observation: substrate-tool-drift
- ~/commit-health flags werks-missing for the now-obsolete persistent-werk model (#2913 retired)
- Class: tools that report against now-obsolete declared state
- Lipstick-on-self risk: keeping these flags around erodes trust in the tool's signal
- Probably warrants Silas-side card to update commit-health to match current model (or retire if commit-health is itself superseded)

## CHORUS bucket (10 cards) — complete

- #2783 — chorus_commit loses staged state — won't-do — superseded by werk-commit v2 (atomic commit-only retires bundling)
- #2784 — TDD gate misclassifies test-prefix — won't-do — already-done-not-closed (source fix #2740 deployed since)
- #3002 — Nightly suite triage + trend nudge — won't-do — partial done; pain-rollup view shipped earlier this week; remaining trend-nudge sliver refile-fresh if wanted
- #2694 — chorus_commit staged-deletions in paths arg — won't-do — superseded by werk-commit v2 (git add -A in ephemeral werk)
- #2785 — Context-synthesis gate cwd recognition — won't-do — chorus-hooks redeployed routinely; stale spec
- #2788 — git-queue.sh push --force-with-lease typed escape — won't-do — superseded by werk-push v2
- #2887 — Replace 3 tautological scenarios in acp.feature — won't-do — superseded by werk v2; integration concern migrates to #3064 e2e card
- #2888 — cdhash-stability BDD with real Mach-O fixture — won't-do — runtime check exists in werk-deploy v2 identity-verify (#3092)

## Patterns from CHORUS bucket

- **6 of 10 cards superseded by werk v2.** Chorus subproduct cards heavily concentrated on chorus_acp / chorus_commit / git-queue.sh — the OLD substrate that v2 retires. 60% supersession rate just in this bucket.
- **2 of 10 already-done-not-closed.** Substrate redeploys (chorus-hooks rebuilds) did the work; cards never moved to Done. The bug exists, then gets fixed adjacent-card, the card sits with a now-false framing.
- **1 of 10 partial-done.** Big card whose substantive value landed across multiple smaller pieces; the explicit AC items aren't all closed but the underlying need is.
- **1 of 10 runtime-check-now-exists.** BDD scenario filed as defense-in-depth; runtime enforcement landed structurally, reducing the BDD scenario's leverage to defense-in-depth-of-defense-in-depth.

Net: CHORUS bucket goes from 10 to 0 Kade-owned active cards.

## Reduction tally so far

- Closed won't-do: #2441, #2434, #2753, #2781, #2783, #2784, #3002, #2694, #2785, #2788, #2887, #2888 = **12**
- Reassigned: #2158 → Wren = **1**
- Total reduction this session: **13 cards**
- Approx remaining: ~94 (from 107)

## Substrate cards implied (running list)

- **Find→Fuseki data pipeline** (implied by #2434, #2441; collapses scanner→SQLite→Fuseki accretion)
- **Code/test/subdomain/alias as first-class graph instances** (implied by #2434)
- **commit-health tool update for ephemeral-werk model** (implied by #2781; substrate-tool-drift class)
- **Daily-trend nudge against the pain-rollup view** (implied by #3002 partial close; small sliver if wanted)
- **e2e demo verification card under #3064 lineage** (was already named; reinforced by #2887 absorbing the integration-test concern)

## Open observations

- **WERK subproduct cards probably have similar concentration.** Next slice to walk through is likely also heavily werk-v2-supersession. Will verify when we get there.
- **The substrate-tool-drift class is real and likely produces more cards.** Tools (like commit-health) reporting against now-obsolete declared state generate false-flag noise that erodes trust in the tooling. Worth surfacing as a class to Silas.
- **Already-done-not-closed pattern needs a mechanism.** The bug got fixed; the card never updated. If chorus-hooks redeploys automatically closed cards whose AC was a "deploy + verify" sequence, this pattern wouldn't accumulate. Not a card to file now, but a substrate observation.

## CLEARING bucket (4 cards) — complete

- #2322 — Surface Clearing+Pulse as sub-sequences under CHORUS — won't-do — obsoleted by Athena v2 promoting Clearing to standalone product (different model, framing no longer fits)
- #2329 — Widen CHORUS_DOMAINS classifier — reassigned to Wren — verified live + real (CHORUS_DOMAINS still = ['chorus','roles','borg']); Wren to check whether Athena v2 model changes the right answer
- #2596 — Role panels show WIP not role-state — reassigned to Wren — partial structural support via #2467 (role-state retired card-tracking); Wren to verify UI residue
- #3006 — Cap message/stream history retention — reassigned to Wren — live perf work, no supersession

### CLEARING bucket pattern (different from CHORUS):

- 1 of 4 obsoleted by Athena v2 product reshape (not werk-v2 substrate)
- 0 of 4 superseded by werk-v2 (Clearing isn't built on chorus_acp / chorus_commit / git-queue.sh — the OLD substrate v2 retires)
- 3 of 4 real-still — either entirely live or partially-done with residual UI work
- All 3 real-stills reassigned to Wren per Athena v2 ownership

**Different bucket → different supersession profile.** Suggests the prune pass needs to consider each subproduct's relationship to the substrate moves:
- CHORUS subproduct → heavily werk-v2-supersession (was built on the OLD substrate)
- CLEARING subproduct → heavily Athena-v2-shift + minor Athena-v2-shift (different reshape applies)
- WERK subproduct → likely werk-v2-supersession heavy (similar to CHORUS)
- LOOM / BORG / ATHENA / CONVERGENCE → likely different reshape lenses

## Reduction tally

- Closed won't-do: now 14 (CHORUS 10 + #2441 + #2434 + #2322 = 14? wait): #2441, #2434, #2753, #2781, #2783, #2784, #3002, #2694, #2785, #2788, #2887, #2888, #2322 = **13 closed won't-do**
- Reassigned: #2158 (Wren), #2329 (Wren), #2596 (Wren), #3006 (Wren) = **4 reassigned**
- Total cards reduced from Kade's queue this session: **17**
- Approx remaining Kade-owned: 107 - 17 ≈ **90**

## WERK bucket — slices A + C (21 cards) — complete

### Category A: OLD MCP-based werk cutover plan (14 cards, all won't-do)
- #2976 (P5 Athena chorus:Service) / #2977 (P6 icd-pipelines.ttl) — Athena service-abstraction never materialized; v2 used filesystem binaries
- #2978 (P7 chorus_push MCP) — superseded by werk-push v2 binary
- #2979 (P8 chorus_approve MCP) — superseded by werk-accept v2 binary (DEC-048 codified)
- #2980-#2985 (C1-C6 /werk/* skill+act jobs) — superseded by binary equivalents called directly by acp.yml; skill+act wrapper layer wasn't built
- #2986/#2987 (C7a/C7b shadow-mode + cutover-flip) — actual path was /acp-v2 sibling skill (coexistence), not shadow-mode
- #2988 (C8 /werk/acp orchestrator) — superseded by act-orchestrated acp.yml (#3064)
- #2989 (M Master sequencing) — cutover happened via different sequencing #3045 → #3056 → #3057 → #3061 → #3062 → #3064

### Category C: chorus_acp/commit/git-queue.sh bug-fix cluster (7 cards, all won't-do)
- #2655 git-queue → MCP single-path — bypassed via bash → binary
- #2719 do_checkout polish — git-queue.sh checkout dropped by #2914 + werk-pull v2
- #2914 /pull ephemeral worktree — done by #2913
- #2918 retirement-gate bats for worktree guards — guards gone, bats moot
- #2920 MCP-affordance multi-card refactor — resolved by ephemeral werks (#2913) + role-state retirement (#2467)
- #2929 full-flow log query card_id consistency — addressed by #3023 + #3063 trace propagation
- #2936 chorus_commit misclassifies pre-commit — chorus_commit MCP on retirement

### Pattern: "Plan-shipped-differently"

The Category A cards reveal a distinct sub-class within werk-v2-supersession:
- Cards filed AGAINST a planned architecture (the P0-P8 MCP-cutover plan)
- Actual implementation chose a DIFFERENT architecture (atomic binaries + act-orchestrated workflow)
- Original intent landed; original SHAPE didn't
- Cards close as superseded-by-different-implementation

This is a cleaner pattern than lipstick-on-pig (where the underlying substrate was broken). Here the substrate was fine; the PLAN about how to evolve it just got revised mid-flight by emerging better architecture. Plan-cards become stale when the team learns from doing.

Suggests: planning cards filed too far ahead become liabilities. The shipped architecture is the answer, not the plan-card.

## Reduction tally

- Closed won't-do this session: 13 + 21 = **34**
- Reassigned: 4
- **Total reduced: 38 cards** (from 107 → ~69 remaining)


## Reassigns to Silas (3 cards)

- #1216 Werk Flow Metrics tab → Silas (observation layer; intent real, scope stale; refile against post-Athena-v2 substrate)
- #1217 Werk Contract tab → Silas (observation layer; intent is the team operating contract — today's principle/policy/practice three-layer work IS this, but bigger; refile against registry)
- #1218 Werk Instruments tab → Silas (observation layer; intent real (eventloop, pulse, crawler, time-budget surface) but AC specifics stale)

### Pattern: "Important-intent-stale-scope"

A new disposition shape: the WHY is real and worth carrying forward, but the WHAT (AC specifics) was written against an old substrate. Reassign to the right owner with explicit "refile fresh when pulling" guidance rather than closing.

Distinct from:
- won't-do-substrate-broken (lipstick): intent + substrate both wrong
- won't-do-superseded (plan-shipped-differently / direct-v2-supersession): intent fulfilled differently
- reassign-as-live: intent + scope both currently right

Important-intent-stale-scope sits at the boundary: keeps the wisdom about what mattered, drops the implementation plan that didn't survive contact with reality.

## Reduction tally

- Closed won't-do this session: 34
- Reassigned: 4 (Wren) + 3 (Silas) = **7**
- **Total reduced from Kade's queue: 41 cards** (from 107 → ~66 remaining)

## BDD cluster — important-intent-stale-scope, OWNER STAYS Kade (3 cards)

- #1798 (40 @wip scenarios in 5 areas) — won't-do; per-domain shape replaces narrow-area shape
- #2813 (BDD tier audit + green on 183 scenarios) — won't-do; audit-against-drifting-scope is lipstick
- #2921 (verification re-run 2026-05-13/14 blockers) — won't-do; blockers were on persistent-werk model retired by #2913

### Future intent (carried forward without a card)

**BDD coverage across 40+ chorus domains, ULTIMATELY** (Jeff 2026-05-27).

Per-domain BDD scenarios as boundary-tests, where each scenario IS a practice-instance enforcement at the domain boundary. Backed by:
- find→Fuseki + queryable subdomain inventory (substrate Jeff and Kade discussed today)
- Practice registry (cybernetic principles + technologist principles + practice IRIs with cite-up edges)
- /werk/code-review verb invoking BDD as boundary-tests for the surfaces being changed

Connects to:
- chorus:loom-principles-principle-tests-hermetic-by-default-integration-gated-explicitly (BDD = integration tier)
- chorus:practice-test-boundary-bugs-live-at-interfaces (BDD targets boundaries)
- chorus:practice-test-witness-spine-events (BDD assertions read spine, not internal state)
- chorus:practice-test-shape-positive-negative-witness (Wren's pattern from #3098/#3100)

The BDD work doesn't disappear; the implementation strategy resets. Refile per-domain fresh when substrate ready.

### Pattern variant: "Important-intent-stale-scope, owner stays"

Distinct from the Silas-reassign version. When the intent is real AND in the original owner's lane, close the stale-scope card BUT explicitly carry the intent forward in notes/memory rather than as a stale card.

Useful when the future work is too speculative or substrate-dependent to file as a new card today, but real enough that the intent shouldn't be lost.

## Reduction tally

- Closed won't-do: 37
- Reassigned: 7
- **Total reduced: 44 cards** (from 107 → ~63 remaining)

## Werk-bucket strong-won't-do batch (14 cards, all won't-do)

- #2073 Werk 5-layer restructure — plan-shipped-piecemeal (Athena v2 + principle/policy/practice + ephemeral werks + pulse/clearing as products)
- #2552 done-gate evidence — done by DEC-2910 (single-evidence rule, demo:preflight-pass card comment)
- #2656, #2657 known-fails/required-checks-drift → MCP — bash→binary bypassed bash→MCP intermediate
- #2663-#2667 v3 substrate plan — plan-shipped-differently via #2913 + werk v2 + hooks
- #2878 builder self-accept refusal — done by werk-accept v2 #3057 (DEC-048 codified in can_accept)
- #2919 role-state multi-card model — done by #2467 (role-state retired card-tracking)
- #2295, #2296, #2297 Chorus-extraction children — done by reality (Clearing UI + tests already in chorus repo)
- #3004 git commit on canonical main allowed — done by canonical-write-guard hook

## Reduction tally

- Closed won't-do this session: 51
- Reassigned: 7
- **Total reduced: 58 cards** (from 107 → ~49 remaining)

## Athena v2 routing schema corrections (Jeff 2026-05-27)

- **Valid subproducts (6, not 7):** athena, loom, werk, borg, convergence, clearing
- **Pulse routes as `subproduct:clearing`** — Pulse IS the structured team state that The Clearing UI renders; same subproduct line. Don't invent `subproduct:pulse`.
- **No `subproduct:chorus`** — chorus is meta-level only; `sequence:chorus` is the catch-all for meta-coordination work without a specific subproduct.
- **Cards CLI enforces valid subproduct values** — `cards set <id> subproduct=X` rejects unknown X with the valid list.

## Corrections to earlier session artifacts

- `/tmp/cybernetic-systems-chorus.html` — claims 7 chorus subproducts including Pulse; should be 6 with Pulse folded into Clearing
- Earlier card walk-throughs mentioned Pulse as standalone; same correction
