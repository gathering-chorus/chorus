# #1643 Merge Spec — Implementation Notes for Kade

**From:** Silas
**Date:** 2026-03-24
**Card:** #1643 (accepted) → feeds #1644 (canonical rebuild via NiFi)

## The Spec

`architect/docs/merge-specification-photos.html` — open it, read sections 3, 6, and 11. That's your contract for the canonical rebuild.

## Watch Items

Three areas where the spec is directional but needs your validation:

1. **NiFi Jolt specs (Section 11)** — the Jolt example for Era 3 is illustrative, not tested. The fallback chain via `modify-default-beta` with string interpolation may not behave as written. Spike one Jolt spec against real Fuseki data before building all four.

2. **Record matching ambiguity (Section 7)** — Tier 3 match (filename only) can produce false positives when the same `IMG_NNNN.jpg` appears across years. You need a resolution rule: if Tier 3 match but dateTaken diverges by >30 days, route to dead-letter for manual review rather than auto-merging.

3. **Validation counts (Section 10)** — the ~100K canonical estimate is rough arithmetic. Run actual counts after first merge pass and report back. If the number is significantly off, the matching tiers need tuning.

## Dependency Chain

1. #1663 (Silas — NiFi observability) must complete before you start #1644
2. NiFi reads three source graphs from Fuseki via SPARQL — confirm graph URIs match what's in Fuseki before building processors
3. Dead-letter queue on Bedroom: `/data/nifi/dead-letter/photos/` — create the directory as part of NiFi flow setup

## Key Design Decisions Already Made

- Era boundaries: 2006, 2013, 2020
- Golden source shifts from Apple (Eras 1-3) to iPhone (Era 4)
- Location has field-level overrides per era (Takeout GPS in Era 2, iPhone GPS in Era 3)
- Merge-all fields: albums, isFavorite, people, hasSourceRecord
- Face clusters do NOT merge across iPhoto→Photos.app boundary (2015-04)
- dateTaken rejection gate: within 7 days of harvest date = suspect (Takeout bug)
