# Kade — Next Session (2026-06-13 EOD reboot)

## Landed today
- **#3397 LANDED** (merge 69bfd7c0): flow-report 55-year-cycle fix + server.ts setInterval `.unref()` + **werk.yml test-step rewrite** (the recurring false `tests:fail` — it ran cargo+jest from the werk ROOT where neither could run; now per-changed-crate cargo + per-changed-package jest, fail-loud, lockfile-guarded). Memory saved: `project_3397_test_floor_fixed_and_canonical_werkyml`.
  - KEY non-obvious fact: the pipeline runs **canonical** werk.yml via `-W`, so a pipeline-fix can't validate on its own run (bootstrap deadlock). PROOF the test-step fix works = the NEXT card's cockpit reads true. VERIFY ON NEXT CARD.
- **#3190 re-scoped** (not pulled): werk-test = promote the now-working inline test step to a real verb + flip advisory→BLOCKING + add a bootstrap-escape for pipeline-fixing cards. OPEN Q (Jeff's dep): does the git-diff heuristic suffice or is the owl-api tests-domain-API still the intended source? Don't pull before resolving.

## #3361 — IN PROGRESS (whole UI extraction, all roles, one sweep)
Re-scoped from per-role split → **the whole chorus UI out of gathering** (Jeff: "my goal is get chorus out of gathering"). Borderline `/system/*` (users/replay/docs) stays in gathering (Jeff's call; system/docs is gathering's own about-docs).

**Inventory LOCKED from gathering's route table (src/app.ts) — trustworthy source, NOT a filename grep:**
~12 chorus pages: `/loom`(+`:role`), `/flow`, `/werk`, `/chorus`(+`/chorus/system`), `/harvesting/icd|convergence|mapper`, `/harvest-manifests`, `/chorus-model-data`, `/borg-assessment`, `/model-data`, `/ontology-views/:domain`. (~50 other routes = Jeff's personal content — stay.)

**Work split (route table revealed it):**
- 3 STATIC (hand-move): werk-process, nifi-doc, instance-explorer.
- 9 SERVER-RENDERED dashboards = **GENERATION target** (owl-api fan-out, Wren's lane) — hand-porting would be throwaway. NOT hand-moves.

**DONE (uncommitted): 2 static pages moved end-to-end**
- werk-process.html + nifi-chorus-integration-design.html → copied to chorus `building/products/werk/` + `building/products/convergence/`; added `/building` static mount in `platform/api/src/server.ts`; **test green** `platform/api/tests/page-moves-3361.test.ts` (3/3, red→green).
- Gathering rip-out COMPLETE for both: deleted files + removed werk-process from page-registry (×2), doc-catalog (about.handler), doc-chrome nav, nav-tree.json; added 3× 301 redirects → chorus building/ homes. tsc clean, nav-tree valid JSON, only intended refs remain.
- STATE: chorus side in werk `chorus-werk/kade-3361` (UNCOMMITTED); gathering side in `jeff-bridwell-personal-site` working tree (UNCOMMITTED). Nothing live yet.

**NEXT on #3361:**
1. instance-explorer (3rd static) — host ambiguous: only ref is km-wrong-cabinet-redirects.ts; no view loads it; likely pairs with rendered `/chorus-model-data` (→ generation group, not a clean standalone static). RESOLVE host before moving.
2. The 9 rendered pages → coordinate with Wren's owl-api generation legs (don't hand-port).
3. Repoint the ~6 genuine `:3000`→`:3340` API refs (codebase-topology, domain-facets, chorus-tests handlers + tests, doc-inventory) — needs PER-REF judgment (some are legit gathering calls); do NOT blanket-sweep. Groups ①(prose) ②(SPARQL pod-graph URIs) = LEAVE.
4. CI grep-gate (AC5) — needs a whitelist for ①②.
5. Commit/land both sides.

## Gate lessons (today)
- test-quality gate (#2196): a test passes if its body invokes an imported production symbol — `app.listen(0)` in the test body is the legit way (real server start), not gaming.
- Wren's #3218 nudge-drain INVERSION landed (PreToolUse BLOCKS every tool but the reply until you answer). Over-trapped on system/mcp nudges → hotfixed peer-only.

## Jeff-mood note
Brutal churny day. He hates: grep dumps, me deciding for him, 100-word responses, asking-not-doing. Wants: trustworthy sources (route table not filenames), him steering scope, brevity, just do the work.
