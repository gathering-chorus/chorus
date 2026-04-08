# Brief: Gallery Data Architecture — What's Actually in Fuseki (and What's Not)

**From:** Silas (Architect) → **To:** Kade (Engineer)
**Date:** 2026-02-25
**Re:** #363 — Wire sexuality collection to search

## The Gap

Your gallery search path goes: `GalleryService.listImages()` → HTTP proxy → images-api (192.168.86.242:8081) → MongoDB. This gives you ~21K images from whatever the remote API returns.

**There is no gallery/media data in Fuseki right now.** I ran a harvest test on Feb 23 (card #224) that loaded 13,465 items from one volume as a collection-graph. That data was lost when containers were recreated during deploys. Current Fuseki: 4,462 graphs / 219,645 triples — all music, photos, notes, books. Zero media/gallery.

## What We Proved (Feb 23)

MongoDB on secondary Mac has **1,848,104 items** (93% JPEG, 5% MP4, 2% ZIP) across 29 external USB volumes. I tested two ingestion approaches:

### Graph-per-entity (REJECTED)
- 10K items → 10K named graphs → existing queries degraded 300-600%
- Extrapolated to 1.85M: page loads would take **minutes**
- TDB2 graph index doesn't scale past ~50K named graphs for cross-graph queries

### Collection-level graphs (VALIDATED)
- 13,465 items → 1 graph (per-volume sharding) → **zero degradation** on existing queries
- Writes: 2,170 items/sec (121x faster than graph-per-entity)
- Full harvest estimate: ~14 minutes for all 1.85M items
- ~29 graphs total (one per physical volume)

**Architecture decision: volume-sharded collection graphs.** One named graph per source volume, all items as triples within that graph.

## What This Means for #363

You have two data paths for gallery:

1. **HTTP proxy (current)** — live from images-api, ~21K items, depends on secondary Mac being up
2. **Fuseki RDF (not yet populated)** — would hold all 1.85M items as persistent triples, queryable via SPARQL, no remote dependency

For search, option 2 is better long-term — it's local, fast, and doesn't fail when the secondary Mac is off. But the harvest pipeline needs to be re-run to populate it.

## Graph URIs (when populated)

```
https://jeffbridwell.com/pods/jeff/media/VideosNew         → ~1.7M items
https://jeffbridwell.com/pods/jeff/media/VideosRilez-Ta    → ~13K items
https://jeffbridwell.com/pods/jeff/media/VideosCoco-Eliza  → ~9K items
... (~29 graphs total, one per physical volume)
```

## RDF Shape (per item)

```turtle
<https://jeffbridwell.com/pods/jeff/media/{id}>
    a jb:Photo ;  # or jb:Video
    jb:photoFilename "filename.jpg" ;
    jb:filePath "/Volumes/VideosNew/..." ;
    dc:format "image/jpeg" ;
    jb:fileSize 39310 ;
    dc:created "2023-11-24T15:50:33Z"^^xsd:dateTime ;
    jb:imageWidth 1920 ;
    jb:imageHeight 1080 ;
    jb:sourceVolume "VideosNew" .
```

## SPARQL Query Pattern

To query media items from collection-graphs (once populated):

```sparql
SELECT ?item ?filename ?format ?size WHERE {
  GRAPH ?g {
    ?item a ?type ;
          jb:photoFilename ?filename ;
          dc:format ?format ;
          jb:fileSize ?size .
    FILTER(?type IN (jb:Photo, jb:Video))
  }
  FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/jeff/media/"))
}
```

This targets only media graphs (not music/photos/notes/books) and runs in sub-second even at 1.85M items because Fuseki scans ~29 collection graphs, not 1.85M entity graphs.

## UPDATE: Fuseki Is Now Populated (2026-02-25)

**Card #376 complete.** Full harvest ran successfully:

| Metric | Value |
|--------|-------|
| Content items | 1,848,104 |
| Models | 22,735 |
| Named graphs | 26 content + 1 models |
| Total triples | 13.07M media + 220K existing = 13.29M |
| Music query perf | 231ms (no degradation) |

**The SPARQL path is now viable for #363 search.** All 1.85M items are queryable via the collection-graph pattern shown above. The HTTP proxy path still works for live data, but Fuseki now has the complete catalog.

Harvest script: `architect/scripts/harvest-media.sh` (list, harvest, verify commands).

## Recommendation

For #363: you now have **two working data paths**. The SPARQL path gives you 1.85M items locally (no remote dependency). The HTTP proxy gives you live data from images-api when it's running.

For #377 (sexuality collection page): use SPARQL queries against `GRAPH ?g { ... } FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/jeff/media/"))` to build the model gallery and content drill-in.

## Reference

Full architectural analysis with benchmarks: `architect/briefs/2026-02-23-images-api-harvest-architecture.md`
