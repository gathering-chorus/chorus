# Brief: Spike — Search in Gathering

**From**: Wren (PM) → Kade (Engineer)
**Date**: 2026-02-21
**Card**: #115
**Priority**: P2 (spike — time-boxed exploration)
**Exit criteria**: Know what's feasible, what's hard, what's the smallest useful version.

## Context

We tried to search Jeff's photos for "garden" images from outside the app. It exposed a real gap: Gathering stores a lot of data (photos, music, books, property, ideas) but has no way to search by meaning. You can browse, paginate, filter by album — but you can't ask "show me garden photos" or "find jazz albums from the 90s."

This spike answers: what would search look like in Gathering, and what's the right scope?

## Two Levels of Search

### Level 1: Search within a collection
Each collection (photos, music, books) gets its own search. User stays in the collection view, types a query, gets filtered results. This is the simpler version and probably ships first.

| Collection | Data source | What's searchable today | What's missing |
|-----------|------------|------------------------|---------------|
| **Photos** | Apple Photos SQLite (CQRS read) | Filename, date, album | Content/scene search — Apple's ZSCENECLASSIFICATION uses opaque numeric codes, ZKEYWORD is empty unless user manually tags |
| **Music** | Apple Music SQLite | Album, artist, genre | Lyrics, mood, decade filtering |
| **Books** | SOLID pod (Turtle/RDF) | Title, author | Subject, location, notes |
| **Property** | SOLID pod | Address, rooms | Photo content within property |
| **Ideas/Projects** | SOLID pod | Title, description | Full-text across notes |

### Level 2: Search across Gathering
A single search box that queries all collections. "garden" returns: garden photos, gardening books, property photos tagged with garden areas, ideas about garden projects. This is the ambitious version — needs a unified index.

## Technical Questions to Answer

1. **Apple Photos scene codes**: ZSCENECLASSIFICATION stores numeric IDs (1, 2, 3... 44+). Can we reverse-engineer the mapping? Apple's Vision framework (`VNClassifyImageRequest`) returns human-readable labels — are these the same codes? Could we run a one-time classification pass on harvested photos to build our own label index?

2. **Photo keyword gap**: ZKEYWORD table is empty. ZGRAPHLABEL has codes but no aliases. Is there a way to populate keywords programmatically, or do we build our own tagging layer?

3. **Local AI classification**: Given Jeff's concentric trust model (photos = middle ring, not cloud), could we use a local model (Core ML, Ollama vision) to classify photos on the Mac? What's the performance cost on the M1?

4. **Existing search patterns**: Music already has genre/artist filtering. Books have title/author. What's the common pattern? Is it just full-text search per collection, or do we need faceted search?

5. **Unified index option**: SQLite FTS5 works well for the Chorus context index (14,700+ messages). Could we build a similar index across collections? What goes in it — metadata only, or actual content?

6. **API auth for internal use**: The photo API requires session auth. For internal tools (like the Clearing background picker), do we need a service-to-service auth path, or should search be a service-level capability (not HTTP)?

## My Recommendation

Start with Level 1 — search within photos. It's the collection with the most data, the worst searchability, and the clearest gap. Smallest version: full-text search on filename + date + any metadata we can extract. If Apple Vision classification is cheap to run locally, that's the big unlock — it turns opaque numeric codes into "garden," "sunset," "portrait."

Don't build Level 2 (cross-collection) until Level 1 proves the pattern in at least 2 collections.

## Time Box

This is a spike — 2-3 hours max. Deliverable: a findings doc with answers to the 6 questions above, plus a recommendation on what to build first. No code unless something is trivially quick to prove out.
