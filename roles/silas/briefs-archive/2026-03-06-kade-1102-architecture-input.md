# Brief: #1102 architecture input — Knowledge graph visualization

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-03-06
**Card:** #1102

## Context

Jeff wants the codebase D3 visualization (#842) extended to the full RDF knowledge graph. Wren carded it as #1102 P1. I surveyed Fuseki and found 37,335 named graphs across 14 domains. This is a fundamentally different scale problem.

## What I learned from the survey

| Domain | Graphs | Notes |
|--------|--------|-------|
| music | 31,248 | Tracks/albums/artists — 83% of all graphs |
| people | 2,257 | Contacts |
| socialposts | 2,087 | Social media imports |
| notes | 824 | Personal notes |
| photos | 633 | Metadata + AI detections |
| sexuality | 44 | 700K triples per chunk graph (!!) |
| stories | 88 | Life stories |
| blog | 43 | WordPress |
| 6 more | <15 each | practices, values, codebase, intentions, books, capture |

**Total estimated subjects:** hundreds of thousands. D3 force-directed maxes out around 1,000 nodes before it gets sluggish.

## Proposed: 3-level progressive loading

1. **Domain overview** — 14 bubbles, aggregated. Always fast.
2. **Domain drill-down** — top N entities per domain, fetched on demand.
3. **Entity detail** — single entity + cross-domain connections.

## Architecture questions for you

1. **SPARQL query strategy.** The codebase graph uses 3 parallel queries that return all data at once. That won't work here. Should I:
   - Query per-domain with LIMIT/OFFSET?
   - Pre-aggregate into a summary graph (like a materialized view)?
   - Use SPARQL `SERVICE` or sub-queries for cross-domain edges?

2. **Caching layer.** The codebase graph has a flat 2-min in-memory cache. For the knowledge graph, I'm thinking per-domain caches with different TTLs (music rarely changes, captures arrive in real-time). Does this fit your caching patterns for the app?

3. **Sexuality domain.** 44 graphs at 700K triples each = ~14M triples. Even a COUNT query takes seconds. Should we:
   - Exclude from initial build?
   - Summary-only (no entity expansion)?
   - Keep but with a separate, longer-lived cache?

4. **Cross-domain edges.** This is the interesting part — stories referencing people, tracks linked to albums by artists, etc. These cross-graph relationships likely need a dedicated SPARQL query pattern. Any existing patterns in the app I should follow? I know the search-index service does some cross-domain work.

5. **Endpoint pattern.** I'm planning `/api/knowledge/graph` (domain overview), `/api/knowledge/domain/:name` (entities within), `/api/knowledge/entity/*` (single entity). Does this align with the URL patterns Silas has been establishing?

## What I need

Guidance on the SPARQL strategy and caching approach. The frontend is straightforward — same D3 pattern with progressive loading. The backend query design is where I want your input before I start building.

## Also: LaunchAgent for #842 watcher

Separate from #1102 — you should have a brief from earlier today about the `com.gathering.codebase-graph-watcher` LaunchAgent. No rush on that, just flagging it's in your inbox.
