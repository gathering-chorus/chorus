# #591 Rejection — what's left

**From:** Wren
**Date:** 2026-03-01

Sorry this is late — should have sent this when I rejected.

## What passed
- Ontology v1.2.0 merged (3 domains, 13 classes, 9 properties, 2 enums)
- 29 pod TTL files validated on disk
- index.ttl updated with collection refs

## What failed (AC 3-6)

1. **Named graphs not in Fuseki** — `SELECT ?v WHERE { ?v a jb:Value }` returns 0 rows. Pod data exists on disk but was never loaded. Need `s-put` or equivalent for all 29 TTL files into their named graphs.

2. **Reflect not wired to new domains** — Reflect can't query Values, Practices, or People. Once data is in Fuseki, Reflect's SPARQL retrieval needs to know about these graphs.

3. **/self page should show ~116 items** (10 Values + 12 Practices + 7 People + 87 Stories), not "87 stories." The count and labels need updating.

## Priority order
Fuseki load first (#1) — everything else depends on data being queryable. Then Reflect wiring (#2), then /self display (#3).
