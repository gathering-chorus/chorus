# Fuseki Performance Audit — #304

**Date:** 2026-02-23
**Author:** Silas

## Dataset Profile

| Metric | Value |
|--------|-------|
| Total triples | 12,515,166 |
| Named graphs | 13,953 |
| TDB2 on disk | 157.6 GB |
| JVM heap max | 2 GB (`-Xmx2g`) |
| JVM actual usage | 1.56 GB (52% of 3 GB container limit) |
| Container memory limit | 3 GB |
| Fuseki version | 5.1.0 |
| Java | OpenJDK 21.0.4 |

### Triple Distribution

| Domain | Triples | % |
|--------|---------|---|
| media/VideosNew | 10,030,609 | 80% |
| media/models | 192,989 | 1.5% |
| media/Videos* (other) | ~680,000 | 5.4% |
| music (albums/tracks/artists) | ~550,000 | 4.4% |
| photos | ~1,050,000 | 8.4% |
| other | ~12,000 | <0.1% |

**Observation:** 80% of triples are from the VideosNew media harvest on the secondary Mac. The music and photos harvests that Jeff uses daily are a small fraction of the dataset.

### Instance Counts (Ontology Classes)

| Class | Count |
|-------|-------|
| MediaPhoto | 988,380 |
| Video | 81,802 |
| Track | 52,174 |
| MediaArchive | 35,009 |
| Model | 20,559 |
| Photo | 9,734 |
| Artist | 7,327 |
| Album | 4,831 |
| FaceDetection | 4,694 |
| Note | 823 |
| PhotoLocation | 328 |
| (28 more classes with <200 each) | |

## Query Benchmarks

Measured against live dataset via direct Fuseki port (3031), bypassing app cache.

| Query | Cold (ms) | Warm (ms) | Notes |
|-------|-----------|-----------|-------|
| Genre list | 167 | ~50 | Fast — small result set |
| Album listing (20 items, paginated) | 1,791 | ~400 | Cross-graph join (album→artist) |
| Album count | 230 | ~80 | Aggregate, single result |
| Total triple count | 4,966 | ~2,000 | Full scan of 12.5M triples |
| Class instance counts (all) | 17,043 | 3,978 | Full scan + GROUP BY + STRSTARTS filter |
| Class instance counts (no filter) | — | 2,887 | Removing STRSTARTS saves ~1s |

### Analysis

1. **The album cross-graph join (1.8s cold) is the page-load bottleneck.** Every music browse page fires this. The 60s app-level cache mitigates repeat hits, but first load after cache expiry is slow.

2. **The class instance count query (17s cold) is brutal.** It scans every triple in every graph. This is the kind of query that should be materialized/cached at a longer TTL, not run on demand.

3. **TDB2 warm cache helps 4-5x.** Second runs are dramatically faster. The JVM is doing its job — but first-hit-after-restart will always be slow with 12.5M triples.

4. **157 GB on disk for 12.5M triples is large.** TDB2 stores multiple index permutations (SPO, POS, OSP, GSPO, GPOS, GOSP, POSG). With 14K named graphs, the graph-aware indexes are significant. This is expected but worth noting for disk budget.

## Tuning Opportunities

### Quick Wins (no code change)

1. **Increase app-level cache TTL for stable queries.** Genre list and class counts don't change often — 60s TTL means they re-query 60 times/hour. A 5-minute or 10-minute TTL for these specific queries would eliminate most cold hits with no staleness risk.

2. **JVM heap is appropriately sized.** 2 GB heap for 12.5M triples is reasonable. The container has 3 GB limit with 1.56 GB actual — no memory pressure. Don't increase unless we add significantly more data.

### Medium Effort

3. **Materialize class instance counts.** Pre-compute and cache the mind-map class count query on sync completion (when Fuseki data changes), not on page load. Store as a JSON file or in a lightweight cache. Eliminates the 17s cold query entirely.

4. **Add a Fuseki text index** for full-text search across titles/names. Currently every search-like query is a SPARQL FILTER with regex — expensive on 12.5M triples. Fuseki supports Lucene-backed text indexes natively.

### Larger Changes

5. **Separate media triples into a dedicated dataset.** The VideosNew media (80% of data, 10M triples) is rarely queried from the app — it's from the secondary Mac's media catalog. Moving it to a `media` dataset would make the `pods` dataset 2.5M triples instead of 12.5M, making every query 4-5x faster.

6. **Consider TDB2 compaction.** The 157 GB on disk may include dead space from updates/deletes during sync cycles. A TDB2 compact operation could reduce disk footprint.

## Recommendation

Start with #1 (cache TTL tuning) and #3 (materialized class counts) — both are low-risk, high-impact. #5 (dataset separation) is the biggest structural win but needs more analysis on how the media data is actually used.

No code changes made in this audit — findings only.

— Silas
