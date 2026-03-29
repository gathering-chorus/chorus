# Brief: #1102 implementation plan — Knowledge graph visualization

**From:** Kade (Engineer)
**To:** Wren (PM)
**Date:** 2026-03-06
**Card:** #1102

## Data landscape (surveyed Fuseki)

The knowledge graph is massive — 37,335 named graphs across 14 domains:

| Domain | Graphs | Scale |
|--------|--------|-------|
| music | 31,248 | Tracks, albums, artists — by far the largest |
| people | 2,257 | Contacts, relationships |
| socialposts | 2,087 | Imported social media |
| notes | 824 | Personal notes |
| photos | 633 | Photo metadata + AI detections |
| stories | 88 | Life stories |
| capture | 73 | SMS/voice captures |
| sexuality | 44 | Collection volumes (700K triples each!) |
| blog | 43 | WordPress posts |
| practices | 13 | Self practices |
| values | 11 | Personal values |
| codebase | 5 | What we just built (#842) |
| intentions | 5 | Goals |
| books | 3 | Reading list |

**Key insight:** Music alone has 31K graphs. We cannot render even 1% of this in D3 at once. The codebase graph worked because 374 nodes is within D3's comfortable range (~1,000 max before performance degrades). This is 100x that scale.

## Proposed architecture: 3-level zoom

**Level 1 — Domain overview (like mind map)**
- 14 domain bubbles, sized by graph count, colored by spoke
- Click domain → expand to Level 2
- Shows cross-domain edge counts (e.g., "stories references 23 people")
- ~20 nodes, always fast

**Level 2 — Domain detail**
- Top N entities within a domain (e.g., top 50 artists by connection count)
- Paginated/filterable — search within domain
- Shows intra-domain structure (artist → album → track hierarchy)
- Click entity → Level 3 detail panel (like codebase node detail)

**Level 3 — Entity detail**
- Single entity with all connections across domains
- Reuses the explain/narration pattern from #842
- "Paulo Dorow is a person connected to 3 stories, 12 practices, and the Reflecting spoke"

## Technical approach

1. **New handler:** `knowledge-graph.handler.ts` — same pattern as codebase-graph.handler
2. **Domain-level SPARQL:** Aggregate query that counts by domain + type + cross-domain edges
3. **Entity-level SPARQL:** Per-domain queries with LIMIT/OFFSET, parameterized by domain
4. **Progressive loading:** Frontend fetches Level 1 on page load, fetches Level 2 on domain click
5. **Caching:** Same 2-min cache pattern, but per-domain caches to avoid loading everything

## What I need from you

1. **AC on card #1102** — confirm the 3-level zoom model matches Jeff's intent
2. **Domain priority** — which domains to make work first? Music is biggest but maybe start with stories/people/practices for a more meaningful demo?
3. **Page placement** — new `/knowledge-graph` page or extend the existing mind map?

## Risk

The sexuality domain has 14M+ triples across 44 graphs. Even counting subjects per graph takes seconds. That domain may need special handling (summary only, no entity expansion) or exclusion from the initial build.

## Timeline estimate

This is bigger than #842. #842 was 4 commits in one session because the data fit in memory. #1102 needs progressive loading, per-domain caching, and a new UI pattern. I'd estimate 2-3 sessions to get Level 1+2 working with a demo-ready domain.
