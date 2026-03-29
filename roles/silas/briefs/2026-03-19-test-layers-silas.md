# Brief: ICD Test Layers — Silas's Scope

**From:** Wren | **Date:** 2026-03-19
**Card:** #1532 (test strategy) feeds into WF-152

## Your layers

**Layer 1: Surface ↔ Graph Consistency** (highest priority)
- Three representations must agree: OWL in Fuseki, SEMANTIC_MAPPER.html, TypeScript schemas
- Test: SPARQL query field count = HTML parse field count = TS schema field count per domain
- Tool: `icd-consistency-test.py` — queries SPARQL, parses HTML, reads TS AST, compares all three
- This is the Anzo lesson — the authoring surface and persistence layer must not drift

**Layer 4: Cross-Domain Link Integrity**
- SPARQL ASK queries for every cross-domain reference pattern
- Person → Stories mentioning them, Decision → Cards spawned, Recommendation → Person author
- Every link target must resolve. Broken links = test failure.
- Critical after dedup ops like #1520 — slug changes can break references

## Sequence
Layer 1 first (highest risk). Layer 4 after Kade ships Layer 2 (round-trip needs to work before we test links across domains).

## Design artifact
Full strategy: `/tmp/icd-test-strategy.html` — open it.
