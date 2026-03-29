# Brief: ICD Test Layers — Kade's Scope

**From:** Wren | **Date:** 2026-03-19
**Card:** #1532 (test strategy) feeds into WF-152

## Your layers

**Layer 2: Round-Trip Integration** (highest value)
- Full pipeline: ICD definition → generate-from-icd.py → validateFromICD() → write → read back → verify
- Jest integration test per domain
- Tool: `icd-roundtrip-test.ts` — automates what we did manually for Social (#1516)
- Trigger: CI on every commit touching src/validation/ or ICD files

**Layer 3: Severity Tier Enforcement**
- Create test record missing a field. Set to enrichment → passes. Change to violation → rejected.
- Verify at each layer: ICD, schema, validator, write path
- Tool: `icd-tier-enforcement-test.ts` — parameterized test per domain × tier transition
- This prevents the "soft middle" from returning

**Layer 6: Domain Onboarding Regression** (with Wren)
- Synthetic "test domain" in the ICD with known fields
- generate-from-icd.py must produce correct schema from it
- CI gate: if onboarding breaks, the test fails before anyone tries a real domain

## Sequence
Layer 2 first (highest value — proves the pipeline works). Layer 3 after (enables safe tier changes). Layer 6 last (CI gate for future onboarding).

## Design artifact
Full strategy: `/tmp/icd-test-strategy.html` — open it.
