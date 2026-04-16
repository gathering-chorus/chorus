# Brief: Generate Social & Professional validation schemas from ICD

**From:** Wren
**To:** Silas
**Date:** 2026-03-18
**Card:** #1516 (WF-148 step 3)

## What

SEMANTIC_MAPPER.html now has a complete `Social & Professional` domain section with 5 consumer ICD types. Generate the validation schemas and wire them into write paths, same pattern as Photos (#1515).

## Types to generate

| Type | Source | Records | Write path |
|------|--------|---------|------------|
| jb:SocialPost | FB + LI merged | 2,075 existing TTL | socialpost-pod.service.ts |
| jb:Person | FB + LI merged | 2,104+ | people write path |
| jb:Recommendation | LI only | 69 (31 received + 38 given) | new — not yet harvested |
| jb:Position | LI only | 10 | new — not yet harvested |
| jb:Endorsement | LI only | 294 | new — not yet harvested |

## Pattern

Same as Photos: declare `SOCIAL_ICD_SCHEMA`, `PERSON_ICD_SCHEMA`, `RECOMMENDATION_ICD_SCHEMA`, `POSITION_ICD_SCHEMA`, `ENDORSEMENT_ICD_SCHEMA` using the violations/warnings arrays from the ICD consumer section. Wire `validateFromICD` into each write path.

## Priority

SocialPost and Person first — they have existing data to validate. Recommendation/Position/Endorsement can follow since they need harvest code first.

## ICD location

`data/about/SEMANTIC_MAPPER.html` → domain `#domain-social` → Consumer ICD section.
