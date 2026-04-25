# Next Session

## Pull immediately
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
