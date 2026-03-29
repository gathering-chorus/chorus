# Content Ingestion Matrix

Last updated: 2026-02-16

## Core Principle

**This system is a semantic memory layer, not a content management system.** Content lives where it lives — Google Photos, Apple Music, Spotify, Facebook, LinkedIn, local storage. The knowledge graph harvests *metadata* — the memory of what Jeff has, when it mattered, and how it connects. The pods own Jeff's relationship to his content, not the content itself.

Most external sources are **harvest targets**: metadata comes in, content stays in the source system. The value isn't in any one source — it's in the **connections across sources** that only exist in the graph. A book that influenced a garden plan. A photo from a trip that connects to a blog post. A song linked to a memory. No single service provides that. The ontology's cross-domain relationships (`relatedTo`, `mentions`, `hasCollection`) are designed exactly for this.

The **WordPress harvester** is the proven template: external system owns content, harvest pipeline maps metadata to the ontology, writes Turtle to pods. Every new source follows this pattern.

---

## Ingestion Depth Levels

- **L0 — Reference only**: URI/pointer stored. Pod knows the source exists but doesn't model individual items. (e.g., "I have a Google Photos library")
- **L1 — Catalog metadata**: Basic descriptive metadata per item (title, date, source URI, type). Searchable via SPARQL. Content stays in source. (e.g., album name, photo date, book title/author)
- **L2 — Rich metadata + relationships**: Full ontology modeling. Personal annotations, ratings, cross-domain connections. (e.g., "this book is on this shelf, rated 4 stars, and inspired this garden project")
- **L3 — Content + metadata**: Actual content imported/mirrored into pods. (e.g., blog post full text, book cover images stored locally)

**Most external sources target L1.** L2 is added selectively where personal meaning matters. L3 is reserved for content Jeff authors or owns directly.

---

## Content Sources

### Native Content (L3 — authored here, stored here)

#### Blog Posts
- **Source**: WordPress (harvested via webhook)
- **Volume**: ~40+ currently, growing
- **Depth**: L3 — full text + taxonomy in pods
- **Current state**: Functional. Webhook pipeline, Turtle storage, collection view.
- **Scale concern**: None. Current pattern works to thousands.

#### Ideas & Projects
- **Source**: Native to system
- **Volume**: Growing organically
- **Depth**: L3 — native content with lifecycle
- **Current state**: Functional. Full CRUD, incubation board, visibility transitions.
- **Scale concern**: None. Unlikely to hit thousands.

#### Property
- **Source**: Physical properties + Google Photos integration
- **Volume**: Small (1-3 properties, dozens of rooms/gardens)
- **Depth**: L2/L3
- **Current state**: Functional. Full CRUD with nested structure.
- **Scale concern**: None.

---

### Owned Collections (L2 — rich metadata, content may be local)

#### Books
- **Source**: Physical collection + Open Library metadata + Claude Vision classification
- **Volume**: 3-5k books
- **Depth**: L2/L3 — rich metadata (shelf location, rating, notes) + cover images for cataloged books
- **Current state**: Upload pipeline functional. 95% test coverage.
- **Question for Jeff**: All 3-5k at L2? Or a curated active set at L2 with the rest at L1 (title/author/ISBN)?
- **Scale concern**: 5k files is on the edge for directory scanning. Workable with partitioning. Fuseki handles 125k triples fine.

#### Local Media (images-api)
- **Source**: Local storage, 200TB+
- **Volume**: Massive
- **Depth**: L1 — catalog metadata. Content stays behind images-api.
- **Current state**: images-api exists, gallery service connects to it.
- **Question for Jeff**: What metadata exists in images-api today? Filesystem paths? EXIF data? Album structure? How much do you want queryable via SPARQL?
- **Scale concern**: Depends on item count and metadata richness. If this is the same 1M+ photos, see Photos below.

---

### Harvested Sources (L1 — metadata harvested, content stays in source)

#### Google Photos
- **Source**: Google Photos API
- **Volume**: Part of 1M+ photos/movies
- **Depth**: L1 — date, album, basic tags, source URI. Photos stay in Google.
- **Current state**: OAuth2 flow exists for property album imports.
- **Harvest pattern**: Google Photos API → harvester → Turtle metadata in pods
- **Scale concern**: At L1, ~5-10 triples per photo. 1M photos = 5-10M triples. Cannot use one Turtle file per photo — needs aggregate pattern (per album or per time period) or Fuseki-primary indexing.

#### Apple Photos
- **Source**: Apple Photos library (local or iCloud)
- **Volume**: Part of 1M+ photos/movies
- **Depth**: L1 — same as Google Photos
- **Current state**: Not built.
- **Harvest pattern**: Apple Photos has no public API comparable to Google. Options: export-based (PhotoKit on macOS), or reference-only via filesystem paths.
- **Scale concern**: Same as Google Photos.

#### Apple Music (NOW — First Harvester)
- **Source**: Apple Music app via JXA (JavaScript for Automation)
- **Volume**: ~4,000 albums, ~66,000 tracks
- **Depth**: L1/L2 — artist, album, genre, year, track number, duration, play count, skip count, artwork. Music stays in Apple Music.
- **Current state**: **IN PROGRESS** (Kade building, card #47 P1). Ontology v0.7.0 shipped (Album/Track/Artist/Genre + HarvestSource/HarvestRun). Multi-source from day one.
- **Harvest pattern**: Apple Music JXA → Node.js ingest → artist normalization + dedup → Pattern B Turtle (per-album) → Fuseki sync. Two-phase: Extract (JXA script, 500-track batches) → Ingest (Node.js, ontology mapping).
- **Scale concern**: 66k tracks = ~660k triples. Pattern B (per-album Turtle) handles this well. First real pressure test for Fuseki write/query performance at scale.
- **Key decisions**: Normalize artist names (Jeff confirmed). Compilations retain album name + per-track artist. Play count yes, ratings no. Composite dedup key: normalize(artist) + album + title + round(duration).

#### Spotify
- **Source**: Spotify API
- **Volume**: Streaming library history
- **Depth**: L1 — artist, album, year, genre, source URI. Music stays in Spotify.
- **Current state**: Not built. Will be second music source (multi-source architecture already designed).
- **Harvest pattern**: Spotify API → harvester → Turtle. Same ontology as Apple Music (v0.7.0 supports multi-source).
- **Scale concern**: Moderate. Streaming history could be large but manageable.

#### Facebook
- **Source**: Facebook (data export or Graph API)
- **Volume**: Unknown — years of posts, photos, connections
- **Depth**: L1 — post metadata, connection list, timeline snapshots. Content stays in Facebook (or export archive).
- **Current state**: Not built.
- **Harvest pattern**: Facebook data export (JSON/HTML) → parser → Turtle. Graph API is increasingly restricted.
- **Scale concern**: Low to moderate. Personal Facebook data is typically thousands of items, not millions.

#### LinkedIn
- **Source**: LinkedIn (data export)
- **Volume**: Unknown — connections, posts, career history
- **Depth**: L1 — professional network metadata, career timeline, post metadata.
- **Current state**: Not built.
- **Harvest pattern**: LinkedIn data export → parser → Turtle. API is very restricted.
- **Scale concern**: Low. Hundreds to low thousands of items.

---

### Future Sources (not yet scoped)

- **Recipes**: From Jeff's global cooking interests. Likely L2 (ingredients, techniques, cultural connections). Volume unknown.
- **Reading notes**: Annotations, quotes from books. L2/L3. Could be large if all 3-5k books get notes.
- **GitHub Projects**: RDF bridge planned. L1 metadata (issues, PRs, project boards). Moderate volume.
- **Streaming history**: Play counts, favorites, playlists. L1. Potentially large over time.

---

## Scale Summary

| Source | Volume | Target Depth | Est. Triples | Storage Pattern |
|--------|--------|-------------|-------------|-----------------|
| Blog | 40+ | L3 | ~5k | Pattern A (one file per post) |
| Ideas/Projects | <100 | L3 | ~2k | Pattern A (one file per item) |
| Property | Small | L2/L3 | ~5k | Pattern A (one file per entity) |
| Books | 3-5k | L2/L3 | 75k-150k | Pattern A with directory partitioning |
| Local Media | 200TB | L1 | TBD (depends on item count) | Pattern B or C |
| Google Photos | 500k-1M+ | L1 | 5-10M | Pattern B (per album) or C (Fuseki-primary) |
| Apple Photos | 500k-1M+ | L1 | 5-10M | Pattern B or C |
| Music (Apple) | 66k tracks | L1/L2 | ~660k | Pattern B (per album) — IN PROGRESS |
| Music (Spotify) | TBD | L1 | TBD | Pattern B — planned |
| Facebook | Thousands | L1 | 10k-50k | Pattern A |
| LinkedIn | Hundreds | L1 | 5k-10k | Pattern A |
| **Total** | | | **~15-25M** | |

---

## Architectural Patterns by Scale

### Pattern A: One Turtle file per resource (current)
- **Works for**: Collections under ~5k items
- **Used by**: Books, blog, ideas, property
- Listing = directory scan, detail = file read, one named graph per file in Fuseki
- Simple, portable, human-readable

### Pattern B: Aggregate Turtle files
- **Works for**: Collections at 10k-100k items where items belong to natural groups
- One Turtle file per album, per month, per category
- Fewer files, fewer named graphs, bulk-loadable
- Trade-off: coarser update granularity (rewrite whole aggregate on change)
- **Good for**: Harvested photo metadata (one file per album), music catalogs

### Pattern C: Fuseki-primary with reference manifest
- **Works for**: Collections at 100k+ items
- Metadata goes directly into Fuseki via SPARQL INSERT (not via filesystem Turtle)
- Pod stores a reference manifest (source URI, last harvest timestamp, item count)
- Content stays in source system
- Trade-off: Fuseki becomes source of truth for this collection's metadata, not filesystem. Breaks the "Turtle files are the source of truth" assumption for this collection.
- **Good for**: Very large harvested collections (1M+ photos)

### Pattern D: External catalog with harvest bridge
- **Works for**: Content with its own management system and API
- Source retains ownership, harvest pipeline imports metadata periodically
- Template: WordPress harvester pattern (already built)
- Trade-off: metadata can be stale between harvests
- **Good for**: All external services (Google Photos, Spotify, Facebook, LinkedIn)

---

## Repeatable Harvest Pipeline

Every external source follows the same pattern:

```
Source API → Harvester Service → Ontology Mapping → Turtle/SPARQL → Pod Metadata + Fuseki Index
```

Components:
1. **Source adapter**: Handles auth (OAuth2, API key, data export parsing) and pagination
2. **Ontology mapper**: Maps source schema to `jb:` ontology classes and properties
3. **Writer**: Turtle files (Pattern A/B) or SPARQL INSERT (Pattern C)
4. **Provenance**: Every harvested resource gets `jb:harvestedFrom` + `jb:harvestedAt`
5. **Incremental sync**: Track last harvest timestamp, only fetch new/changed items

The WordPress harvester is the reference implementation. Each new source is a new adapter + mapper; the writer and provenance patterns are shared.

---

## Fuseki Sizing Implication

At 15-25M triples total:
- Jena TDB2 persistent storage is required (not in-memory)
- Current Fuseki configuration needs to be verified for TDB2
- Complex cross-graph queries (the visibility-scoped queries from ADR-003) will need attention
- Named graph count at Pattern B: manageable (thousands). At Pattern A for all sources: problematic (millions)
- Recommendation: verify Fuseki TDB2 config now; benchmark query performance at 1M, 5M, 15M triples

---

## Design Principle: Tiers are a spectrum, not a box

L1 → L2 → L3 promotion must be **additive, not transformative**. A source should be promotable without rearchitecting.

**What makes this work with RDF:**
- RDF triples are additive by nature. L1 gives a node `title, date, sourceURI`. L2 adds `relatedTo, mentions, personalRating, notes` to the same node. No rewrite, no migration. L1 queries still work at L2.
- Stable URIs from day one. Even at L1, every resource gets a persistent URI that survives promotion. No identifier changes between tiers.

**What would paint us into a corner:**
- Different URI or identifier schemes at different tiers (avoid: use stable URIs from the start)
- Pattern C (Fuseki-primary) is the hardest to reverse — if a collection starts with Fuseki as source of truth, moving back to filesystem Turtle is a migration. Be deliberate about which collections go Pattern C.
- Patterns A, B, and D are all filesystem-based and interchangeable. A → B (individual files → aggregates) is a non-breaking promotion.

**Default recommendation:** Pattern B (aggregate Turtle files) for high-volume harvested sources, even at L1. Keeps filesystem as source of truth, scales to hundreds of thousands, leaves the door open for L2/L3 enrichment. Reserve Pattern C only if Pattern B proves insufficient — and flag it as a conscious, documented trade-off (future ADR).

## Design Principle: Connectedness over accumulation

The risk at scale is a semantic data swamp — millions of queryable nodes with no coherence or use value. The defense is structural.

**How data lakes die:**
- Metadata harvested without relationships. Nodes exist in isolation. Technically queryable, practically inert.
- Ontology grows horizontally (new domains) without depth (cross-domain connections). Graph is technically one graph, functionally many disconnected subgraphs.
- Nobody curates. L1 accumulates automatically; L2 enrichment requires attention. If the system doesn't surface the opportunity, it doesn't happen.

**Structural defenses:**

1. **Connectedness as a quality metric.** The system should measure and surface graph health: orphan nodes (no cross-domain relationships), disconnected subgraphs (entire collections with no links out), thin nodes (L1 records untouched for months). Not errors — opportunities. Dashboard-visible.

2. **Harvesters look for linkage at ingest.** Even at L1, automatic connection candidates: temporal overlap (photo date matches blog post date → `relatedTo` candidate), shared tags/categories across sources, entity matching (same artist in music and books). Every harvester has a "link discovery" phase, not just a "dump metadata" phase.

3. **The AI layer is the curation engine.** This is where conversational AI becomes architecturally important — not a feature, a structural necessity. An AI that sees the graph, notices patterns, and suggests L2 enrichment: "this album was released the year you took these Italy photos, and you blogged about that trip." Jeff confirms or dismisses. The system gets richer through conversation, not manual tagging.

4. **Temporal dimension.** `harvestedAt` captures when metadata arrived. But *when something mattered to Jeff* is the richer signal. "This book was being read during the same period this project was active." Temporal overlap creates connections that pure metadata matching misses. Consider `dcterms:temporal` or a `jb:activeFrom` / `jb:activeTo` pattern for resources with a lifespan.

5. **Cross-domain ontology properties are load-bearing.** `relatedTo`, `mentions`, `hasCategory`, `hasTag` exist for this. They must be actively used — by harvesters, by the AI, by Jeff — or the graph fragments into silos.

**Metric to track:** Percentage of resources with at least one cross-domain relationship. If this drops as new sources are harvested, the graph is accumulating without connecting. That's the early warning.

## Notes

- The system is a semantic memory layer — own the metadata, reference the content
- L1 is the default for harvested sources. L2 is earned by personal significance. L3 is for authored content.
- The WordPress harvester is the template. Every new source is a variation.
- Cross-source connections are where the unique value lives — the graph sees what no single service can
- The Prometheus guardrail: don't build L3 infrastructure for L1 content
- The data swamp guardrail: don't harvest without connecting. Accumulation without coherence is worse than not harvesting.
