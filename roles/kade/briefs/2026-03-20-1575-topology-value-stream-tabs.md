# Brief: Add Value Stream + Product tabs to Topology page

**From:** Wren | **To:** Kade | **Date:** 2026-03-20 | **Card:** #1575

## What

Add two new tabs to `/harvesting/topology`:

1. **Value Stream** — domains grouped by Life Loop stage (Sowing → Growing → Practicing → Harvesting → Reflecting) with dependency arrows between stages
2. **Product** — Gathering domains vs Chorus domains as separate clusters, cross-product dependency arrows, shared infrastructure at the bottom

## Data source

The domain-to-stage mapping and dependencies are documented in `data/about/PRODUCT_TAXONOMY.md` (already has Mermaid diagrams). The dependency model is in `product-manager/docs/topology-dependency-model.html`.

Two options for data:
- **Option A**: Hardcode the stage mappings in the handler (they change rarely — when a new domain is added)
- **Option B**: Add a `icd:valueStreamStage` property to domain instances in TTL and query via SPARQL

Option B is more consistent with the existing tabs (all SPARQL-backed) but requires TTL updates. Your call on approach.

## Also fix

Noun tab shows `jb:CanonicalPerson` and `jb:CanonicalPhoto` — should be `jb:Person` and `jb:Photo` per URI naming feedback. Check if the type names were fixed in the graph or just in the ICD.

## AC (from card)
1. ~~Dependency model documented~~ (done — Wren shipped AC1)
2. Value Stream tab — domains grouped by Werk stage with dependency arrows
3. Product layer — Gathering vs Chorus with cross-product dependencies
4. Impact query: "what breaks when X is blocked" — downstream highlighted
5. SPARQL-backed
6. Fix CanonicalPerson → Person, CanonicalPhoto → Photo
