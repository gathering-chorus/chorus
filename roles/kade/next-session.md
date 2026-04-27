# Next Session

## What landed today (2026-04-27 morning)
- Research synthesis on social contagion + framing in AI/agent systems and humans → `chorus/designing/docs/social-contagion-and-framing-research.{md,html}`. Catalog picks up automatically (chorus/designing/docs is in SOURCE_DIRS at doc-catalog.handler.ts:341).
- Gate:code + gate:quality on Wren's #2508 athena-owner-write; flagged 3 test gaps + recommended 3 helper extractions before second ontology-write endpoint copies the regex+SPARQL approach.
- Reviewed Silas's #2512 sycophancy/labels-null fix; verified #2463 wave 1a did NOT introduce the narrowing (pre-existing).
- Helped on #2509 catalog-source question (no handler change needed — chorus/designing/docs already scanned).

## Open threads from yesterday's pair-2504 (still pending Jeff direction)
- **(F2) seed-pipeline-flow** — interim gated; permanent move-to-personal-site-repo decision pending.
- **(2c) Lazy-load Vikunja in cards/src/config.ts** — substrate card not yet filed. Right architectural answer for the 7 a-vikunja gates to retire.
- **(2a/2b) CI Vikunja access** — secret + ephemeral instance vs. stay gated. Jeff's call.

## Open threads from this session
- Wren's #2508 helper-extraction recommendation (patchSubdomainField, replaceObjectTriple with WHERE-drift fix, multi-line-literal terminator) — her call whether to file or fold.
- Silas appears to be working on lazy-load Vikunja (cards/src/config.ts modified in his tree). If that's (2c), he's on it.

## Open questions surfaced by research doc that may warrant cards
- Nudge-as-injection layer: PreToolUse hooks defend tools; no equivalent on inbound nudges (Greshake gap).
- Cross-model role experiment: one role on a different base model.
- Affect-strip the spine `digest` field; watch for downstream change.
- Forcing functions on human input into shared context (error blast radius = N agents).

Don't file speculatively. Surface to Jeff; he picks.

## Patterns I'm carrying
- Stop deflecting work to teammates via cards.
- Things right ≠ things done. Hidden gates aren't isolation.
- Lying-by-paraphrase: don't quote fabricated self-quotes.
- Run skills end-to-end.

---
## (Older) Pull immediately
- **#2495** — api-boot ESM/CJS reconciliation. Jeff's direction at session end. Owns: audit SDK imports in `platform/api/src/mcp/server.ts` + `transport.ts`, decide CJS-fixed-paths vs ESM migration, restore mcp-roundtrip CI job to `quality.yml`. This is what I should have done before pulling #2487 and what got dressed up as "scope cut" today.

## What landed today
- #2481 — CI ratchet enforcement (lint-ratchet, tsc/jest/cargo matrices, Dependabot, ci-main-red, branch protection on main with 13 required checks). Org transferred WJeffBridwell → gathering-chorus.
- #2487 — accepted in lean form (bats hermeticity + ADR-026 addendum). Real api-boot work absorbed into #2495.
- #2494 — CI Pipeline Service Design. md at `designing/docs/ci-pipeline-service-design.md`, html at `jeff-bridwell-personal-site/public/gathering-docs/ci-pipeline-service-design.html`. Registered as Athena contract entity in pipelines-domain. Silas gate:arch-pass.

## Filed today (Later)
- #2495 — api-boot ESM/CJS reconciliation (Jeff said pull next)
- #2496 — Ratchet baseline drift signal
- #2497 — KNOWN_FAILS allowlist
- #2498 — Trunk-vs-Rulesets cleanup (Silas pulled)
- #2499 — Demo skill adapts for infra cards
- #2500 — Required-checks drift detector

## Loose threads
- HTML for #2494 still uncommitted in `jeff-bridwell-personal-site` — separate repo, hook blocked the chained commit. Just `git add public/gathering-docs/ci-pipeline-service-design.html && git commit` in that repo when picking up.
- Wren never reviewed #2494 (informational only, not blocking acp).
- Dependabot opened 23 lock-update PRs (#2-#24) on initial scan — triage when there's slack.

## Patterns Jeff named today (don't forget)
- "lean scope" can be AC abandonment dressed up — if a card's AC didn't ship, the gate fails, you don't split into a follow-on and call it done.
- Service designs are HTML in `jeff-bridwell-personal-site/public/gathering-docs/`, not raw markdown. Match the canonical template (Promise/Overview/Sub-Domain Interaction/Dependencies 4-layer/Components/Surfaces/Consumers/Gaps/Next Steps).
- Demos: open the page, pause. Don't narrate. Follow the skill literally — feedback nudges per role's domain are mandatory, not optional.
- Match Jeff's energy. He typed 5 words; don't reply with 150.
- Not multitask. Pulling #2494 mid-#2487 cost focus and made both cards weaker.
