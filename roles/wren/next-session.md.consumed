# Next Session — Wren

## This session shipped (2026-04-25, ~5h)

**#2485 loom-decisions reach Phase 1 closed** (`5c00905a wren+silas: acp #2485`):
- 144 chorus:Decision instances in `urn:chorus:instances` with rich schema (migrated 129 from `urn:chorus:decisions` + 15 net-new). Source graph dropped.
- Athena REST: GET/POST/PUT/DELETE on `/api/athena/subdomains/loom-decisions/decisions`; `/api/loom/decisions` 308.
- MCP tools `chorus_decisions_list/get` live; demo verified `chorus_decisions_get adr-026` returns full §a-d.
- Drift test (`platform/tests/decisions-graph.test.sh`) 14/14 PASS.
- Cite-by-ID lint (`platform/scripts/check-decision-paraphrase.sh`) 135 labels, 0 violations.
- Pre-commit hook `check-decision-direct-edit.sh`.
- Page `/loom/decisions.html` (26 ADRs + 114 DECs).
- Lifecycle-gate model rework (Silas): 3-class FACET_CLASSIFICATION (derived/required-authored/optional-authored). lifecycle.done.pass=TRUE for loom-decisions.
- 4 actors + 3 scenarios + 1 contract authored via canonical Athena POST endpoints. Completeness 31%→69%.

**Cookbook v2** (`/loom/cookbook-substrate-class-domain.html`):
- Two-phase model named (Phase 1 data correctness, Phase 2 refactor/placement).
- Move 1 pre-conditions: query existing graph, audit ID collisions, declare URI scheme, declare predicate-name SoT, audit discover-* scan paths.
- Doneness #6: lifecycle gate measures truth via facet classification.
- MUST/MUST NOT/SHOULD/MAY normative-rules formatting (RFC-style; matches principles-reference-impl shape).

**#2492** (DEC ID collision swat, 3 collisions DEC-093/096/101 → DEC-113/114/115) — done.

**Memory banked:** `feedback_data_clean_then_refactor.md` (Jeff's repeated principle: data clean first, then refactor or rewrite a domain).

## Open threads / Phase 2 follow-ons

1. **#2502** [Wren, P1] — Athena domain page render-rule (every section renders, no hiding-on-empty, add Logs section, HERALD_FACETS expansion). 200+ lines stashed in personal-site working tree by Silas at end of #2485. Owner Wren but page lives in gathering — see #2503.
2. **#2503** [Wren, P2, chore] — Athena page product-ownership question (gathering vs chorus). Architecture decision needed. Today's cross-product scope-grow surfaced this.
3. **#2486** [Silas, P2] — Convert root to npm workspaces (ADR-026 §c follow-on, filed today).
4. **#2195** [Silas, P1, sat in Later for a week] — Worktree isolation for three-role concurrent editing. Bit twice today (Wren server.ts 13:30, Silas Move 5 MCP tools 15:32). Retro item now has two concrete instances; pull next.
5. **Phase 2 placement card for loom-decisions** (TBD) — opens after Phase 1 closes per the new principle. Move loom-decisions code/page to right structural home.

## PM-misses today (carry into reflection)

- 2× performative gate-product PASS retracted. Both caught by Jeff.
- 1× cross-product scope-grow (chorus card editing gathering app's domain-detail.js). Caught by Jeff at 16:01. Stashed.
- Schema-invention miss: built thin chorus:Decision schema without querying the existing rich `urn:chorus:decisions` graph. ~30 min rework.
- Branch-entanglement: I was navigator-in-name but tile-tailing-without-commentary; lost ground until Jeff called it out ("does not feel like navigating").

## First action next session

Check status of pulls. If #2502 or #2195 is in motion, navigate. If not, propose pulling #2195 (worktree isolation) — today made the case that further cross-role work without it pays the entanglement tax.

## Friction next session needs to know

- Gathering-app working tree has a labeled stash from Silas at #2485 close — 200+ lines of HERALD_FACETS expansion + Endpoints rename. Don't lose it; it's the implementation #2502 should pull from.
- Vikunja `done_at` is contaminated with bulk-migration timestamp (2026-04-07T16:38:06Z on all done tasks). Use git log of `<role>: acp #<id>` for closure-by-day signals.
- Cookbook v2 footer references this session's PM-misses. Future sessions can link there as concrete cases of the anti-patterns.

## State at close

- WIP: empty (Wren).
- Recent acp: #2485 (5c00905a).
- Tools loaded: chorus_decisions_get/list MCP tools live and verified.
- Memory: feedback_data_clean_then_refactor banked.
