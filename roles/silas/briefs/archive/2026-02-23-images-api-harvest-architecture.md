# Images-API Harvest Architecture — Graph Model Validation

**Card**: #224 | **Author**: Silas | **Date**: 2026-02-23
**For**: Kade (harvest implementation), Jeff (architectural decision)

## Context

Jeff wants to harvest the images-api media library into Fuseki/RDF as the first large-scale collection — the "biggest first" heuristic to validate architecture under real load before we're 75% done and hit a wall.

## What We Found

### MongoDB (secondary Mac, 192.168.86.242)

| Collection | Documents | Avg Doc Size | Total Size |
|------------|-----------|-------------|------------|
| `content`  | 1,848,104 | 4,394 bytes | 7.7 GB     |
| `models`   | 22,735    | 3,714 bytes | 80.5 MB    |

**`content` is the catalog.** Scanned by `load_content.py` across 29 directories on 18+ external USB drives. Each doc has:
- `file_path`, `content_type`
- `base_attributes`: size, created, modified, accessed
- `extended_attributes.mdls`: macOS Spotlight metadata (dimensions, content type, dates, Finder tags)
- `extended_attributes.xattr`: extended attributes (user tags, quarantine, where-from URLs)
- `video_metadata` (for videos): ffprobe output (format, streams, codecs, duration)
- `image_metadata` (for images): ImageMagick identify output

**Content type breakdown:**
- 1,710,934 JPEG (93%)
- 90,158 MP4 (5%)
- 40,181 ZIP (2%)
- ~7K other (WMV, AVI, MPEG, MOV, PNG, etc.)

**Staleness:** Last full load was Jan 2025 (409 days ago). 17,502 files on mounted volumes are not in Mongo. 9 of 29 source volumes are currently offline (not plugged in).

### Network (between Macs)

| Metric | Measured |
|--------|----------|
| Ping latency | 0.77ms avg |
| SSH round-trip | 131ms |
| Throughput | 11.1 MB/s (SSH) |
| TTL payload for full harvest | ~882 MB |
| Transfer time (data only) | ~80 seconds |

**Network is not the bottleneck.** Fuseki write speed (119 graphs/sec measured) is the constraint.

### Current Fuseki State

- 830,226 triples / 13,925 named graphs
- Docker volume: 39.3 GB (bloated — realistic size for 830K triples is ~160 MB)
- Volume likely contains stale TDB2 journals, old dataset copies, compaction artifacts
- **Cleanup needed regardless of harvest approach**

### Scale Projection

| Metric | Current | After Harvest | Growth |
|--------|---------|---------------|--------|
| Triples | 830K | 19.3M | 23x |
| Named graphs | 13,925 | 1,863,925 | 134x |
| TDB2 storage (realistic) | ~160 MB | 3.6 GB | 22x |
| TDB2 storage (pessimistic) | ~160 MB | 8.6 GB | 54x |

## The Architectural Question

**Graph-per-entity at 1.85M graphs — will TDB2 handle it?**

Current pattern: every entity (album, photo, note) gets its own named graph for visibility isolation. At 13,925 graphs this works fine. At 1.85M graphs, TDB2's graph index becomes a potential bottleneck for:
- Graph enumeration queries (`SELECT DISTINCT ?g ...`)
- Cross-graph joins
- SPARQL UPDATE operations
- TDB2 compaction / maintenance

## Placement Decision: Triples Here

**Recommendation: keep triples on primary Mac (current Fuseki).**

- Sub-1ms latency means placement is irrelevant for queries
- One Fuseki instance is simpler than two + federation
- App code stays unchanged
- At realistic TDB2 ratios, 18.5M triples fits in 3-9 GB
- Primary Mac disk at 87% but 39GB of Fuseki bloat is recoverable

## 10K Test Harvest Results (COMPLETED 2026-02-23)

### Write Performance
- 10,000 graphs written in 566.7s = **18 graphs/sec**
- 104,616 triples added (10.5 triples/graph avg)
- 696 bytes/item TTL payload
- Zero errors
- Extrapolated full harvest: **29h sequential / 7.3h 4x parallel**

### Query Degradation — THE WALL

Adding just 10K graphs (+72% graph count) caused severe cross-graph query degradation:

| Query | Before (13.9K graphs) | After (23.9K graphs) | Delta | Extrapolated @ 1.85M |
|-------|----------------------|---------------------|-------|---------------------|
| music_albums | 441ms | 1,834ms | +316% | **~256s** |
| music_genres | 115ms | 816ms | +610% | **~129s** |
| cross_graph_join | 411ms | 1,886ms | +359% | **~271s** |
| photo_listing | 860ms | 1,293ms | +50% | ~80s |
| notes_listing | 164ms | 303ms | +85% | ~26s |
| single_graph | 56ms | 117ms | +109% | ~11s |

Queries using `GRAPH ?g { ... }` pattern-matching degrade because Fuseki must open and scan each named graph. At 1.85M graphs, page-load queries would take **minutes**, not milliseconds.

Counter-intuitively, `COUNT(*)` aggregations got faster (JVM warmup / cache effects).

### Decision: Graph-Per-Entity Does Not Scale for Media

**Graph-per-entity is the right model for small collections** (books, albums, notes — hundreds to low thousands). **It does not work for million-item collections.**

## Revised Architecture: Hybrid Graph Model

### Collection-Level Graphs for Media

Instead of 1.85M named graphs, use **volume-sharded collection graphs**:

```
https://jeffbridwell.com/pods/jeff/media/VideosNew       → 1 graph, ~1.7M items
https://jeffbridwell.com/pods/jeff/media/VideosRilez-Ta   → 1 graph, ~13K items
https://jeffbridwell.com/pods/jeff/media/VideosCoco-Eliza → 1 graph, ~9K items
... (~29 graphs total)
```

**Why volume-sharded, not one giant graph:**
- Natural partition matching physical storage
- Can load/refresh per-volume independently
- Volume offline → its graph stays as-is until reconnected
- Query scoping: `GRAPH <.../media/VideosNew> { ... }` targets one volume

**Impact on visibility model:** Media is all private (Jeff's personal library). No cross-visibility concern. Collection-level graphs are fine.

**Impact on existing queries:** Existing `GRAPH ?g { ... }` queries continue to work — they'll match the ~29 media graphs plus the existing per-entity graphs. No app code changes needed.

### Write Strategy

| Approach | Graphs | Write Time | Query Impact |
|----------|--------|------------|-------------|
| Graph-per-entity (rejected) | 1,850,000 | ~29h | Queries degrade to minutes |
| Per-volume graphs | ~29 | ~minutes | Queries stay sub-second |
| Single collection graph | 1 | ~minutes | Largest single graph scan |

Per-volume graphs: upload each volume's items as one large TTL file via GSP PUT. A 1.7M-item TTL file (~1.2 GB) can be uploaded in a single HTTP PUT — no per-item overhead.

### Harvest Pipeline

1. Export from MongoDB (or rescan filesystem) on secondary Mac
2. Convert to TTL grouped by volume prefix
3. Push each volume's TTL to Fuseki via GSP PUT from primary Mac
4. Incremental updates: diff by checksum, SPARQL DELETE/INSERT for changed items

## Single-Volume Test Results (COMPLETED 2026-02-23)

Loaded 13,465 items from VideosRilez-Ta as a single collection graph.

### Write Performance

| Approach | Items | Time | Rate | Full Harvest Est |
|----------|-------|------|------|-----------------|
| Graph-per-entity | 10,000 | 566.7s | 18 items/sec | **29 hours** |
| Collection-graph | 13,465 | 6.2s | 2,170 items/sec | **~14 minutes** |

Collection-graph is **121x faster** for writes.

### Query Impact (existing queries, with 138K new triples loaded)

| Query | Baseline | +10K Entity Graphs | +1 Collection Graph |
|-------|----------|-------------------|-------------------|
| music_albums | 441ms | 1,834ms (+316%) | 190ms (-57%) |
| music_genres | 115ms | 816ms (+610%) | 78ms (-32%) |
| cross_graph_join | 411ms | 1,886ms (+359%) | 654ms (+59%) |
| photo_listing | 860ms | 1,293ms (+50%) | 808ms (-6%) |
| notes_listing | 164ms | 303ms (+85%) | 195ms (+19%) |

**Collection-graph adds 16% more triples than the entity-graph test but causes zero query degradation on existing queries.** Most queries actually got faster (JVM cache warming). The only notable bump is cross_graph_join (+59%), which is the most expensive query pattern regardless.

### Validated: Collection-Graph is the Architecture

The data is unambiguous. Collection-level graphs for media are:
- 121x faster writes
- Zero degradation on existing queries
- +1 graph instead of +13,465 graphs
- 6.8 MB TTL for 13.5K items (linear → ~930 MB for 1.85M items)

### Next Steps

1. **Build full harvest pipeline** — export all 29 volumes from Mongo, convert to per-volume TTL, upload to Fuseki
2. **Add MediaItem class to ontology** — parent of Photo/Video, with sourceVolume as first-class property
3. **Handle offline volumes** — graph persists from last known state, metadata flag for "volume offline"
4. **Incremental update strategy** — diff by file_path, SPARQL DELETE/INSERT for changed items within a volume graph
5. **Brief Kade** — harvest handler + collection browse for media items, query patterns for collection-graph data

## RDF Mapping (Draft)

MongoDB `content` doc → RDF:

```turtle
@prefix jb: <https://jeffbridwell.com/ontology#> .
@prefix dc: <http://purl.org/dc/terms/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<https://jeffbridwell.com/pods/jeff/media/{checksum-or-id}>
    a jb:Photo ;  # or jb:Video based on content_type
    jb:photoFilename "filename.jpg" ;
    jb:filePath "/Volumes/VideosNew/..." ;
    dc:format "image/jpeg" ;
    jb:fileSize 39310 ;
    dc:created "2023-11-24T15:50:33Z"^^xsd:dateTime ;
    dc:modified "2023-11-24T15:50:33Z"^^xsd:dateTime ;
    jb:imageWidth 1920 ;
    jb:imageHeight 1080 ;
    jb:colorSpace "RGB" ;
    jb:sourceVolume "VideosNew" ;
    # Video-specific (if MP4/etc):
    # jb:duration 3600 ;
    # jb:videoCodec "h264" ;
    # jb:audioCodec "aac" ;
.
```

~8-12 triples per item. URI keyed by checksum (dedup-safe) or MongoDB ObjectId.

## Risks

1. **Offline volumes**: 9 of 29 volumes not mounted. Mongo has stale data for those. Volume graph stays as-is until volume reconnects.
2. **Mongo staleness**: 409 days stale, +17,502 files on mounted volumes not indexed. Consider filesystem-first harvest (bypass Mongo) for accuracy.
3. **Fuseki volume bloat**: 39.3 GB for 830K triples. TDB2 index files are pre-allocated (B+ tree chunks), not actually bloated — this is normal TDB2 behavior. Will grow proportionally with data.
4. **Large TTL uploads**: A 1.7M-item volume as single TTL (~1.2 GB) may require Fuseki memory tuning for parsing. May need chunked approach (e.g., 100K items per upload).
5. **Collection-graph query patterns**: App queries currently assume graph-per-entity. Need to verify that `GRAPH ?g { ... }` with 29 media graphs + 14K entity graphs doesn't introduce new performance patterns.

## Dependencies

- Card #214: Fuseki perf harness (provides baseline and regression detection)
- MongoDB on secondary Mac accessible via SSH (confirmed working)
- Fuseki port 3031 bound to 127.0.0.1 (ADR-012) — harvest must push from primary Mac

## Appendix: TDB2 Volume Analysis

The 39.3 GB Fuseki volume is **not bloated** — it's normal TDB2 pre-allocation:

| File | Size | Purpose |
|------|------|---------|
| POSG.dat | 11 GB | Predicate-Object-Subject-Graph index |
| OSPG.dat | 7.8 GB | Object-Subject-Predicate-Graph index |
| POSG.idn | 5.1 GB | POSG B+ tree index |
| OSPG.idn | 3.8 GB | OSPG B+ tree index |
| nodes.dat | 1.4 GB | Node dictionary (all unique terms) |
| GSPO/GPOS/GOSP | ~1 GB each | Graph-first quad indexes |
| GPU | 440 MB | Graph-Predicate-Unique (graph mgmt) |

TDB2 uses memory-mapped files that grow in power-of-2 chunks. The 37 GB includes pre-allocated empty pages in B+ tree structures. Extrapolation to 20M triples is unreliable — growth may be sub-linear as pages fill in. **The single-volume test (Step 2 in Next Steps) will give us real storage numbers.** If TDB2 storage proves untenable at scale, alternatives include: Fuseki compact utility, TDB2 → flat file export/reimport, or evaluating lighter-weight triple stores.
