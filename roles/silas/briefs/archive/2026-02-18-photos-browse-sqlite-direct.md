# Brief: Photos Browse Page — SQLite Direct Read

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-02-18
**Priority:** P1 — blocking visible UX quality

## Problem

The Photos browse page (`/collection/photos`) currently reads photo data by:
1. Listing album/unalbummed Turtle files from the SOLID pod
2. Regex-parsing each `.ttl` file to extract photo metadata
3. Assembling results in memory, then filtering/sorting/paginating

This creates two concrete problems Jeff can see right now:

**Broken thumbnails:** The Turtle parser constructs a default `thumbnailPath` for every photo (`/thumbnails/photos/{bucket}/{slug}.jpg`) even when no file exists on disk. Result: broken `<img>` tags everywhere instead of clean placeholders.

**Screenshot filter dead:** We added `jb:mediaSubtype` to the harvester and handler, but the existing Turtle files on disk don't have this property yet. The filter shows "9729 photos" with zero filtered — screenshots still dominate the feed. Every data improvement requires a full re-harvest before it's visible.

## Proposal

**Read directly from Apple Photos SQLite for the browse page.** Keep the Turtle/pod pipeline for RDF graph storage and SPARQL queries, but skip it for the browse UI.

### Why this works:
- SQLite extraction already proven: 152ms for all 9,729 assets
- `ZKINDSUBTYPE` is already in the table — screenshot filter works immediately, no re-harvest
- Thumbnail paths can be resolved against actual disk state (check file existence)
- GPS, favorites, albums, face data — all available without Turtle round-trip
- We already have `PhotoSqliteService` with the queries built

### What changes:
- `PhotoHandler.renderCollection()` and `listPhotos()` call `PhotoSqliteService` instead of reading pod Turtle files
- Thumbnail path resolution checks disk existence before setting a path
- Pod/Turtle storage continues to be written by the harvester for RDF/graph use
- `parsePhotosFromTurtle()` stays for album detail views or anywhere Turtle is the right source

### What stays the same:
- Harvester pipeline (SQLite → normalize → Turtle) unchanged
- Pod storage structure unchanged
- SPARQL/Fuseki queries unchanged
- Album detail view can still read from Turtle if preferred

## Architectural Questions for Silas

1. **Read-path split OK?** Browse reads SQLite directly, graph/SPARQL reads Turtle/Fuseki. Two read paths for different use cases. Does this violate any architectural principle, or is it pragmatic separation?

2. **Source of truth concern?** The pod is supposed to be the canonical store. If browse bypasses it, does that weaken the pod's role? Or is the pod's role really about graph relationships and RDF semantics, not serving browse UIs?

3. **Alternative: re-harvest first?** We could just trigger a re-harvest now to backfill `mediaSubtype` and fix thumbnail paths in the Turtle. That keeps the single read path but means every data improvement needs a harvest cycle. Your call on which trade-off is better.

## Impact

If approved: browse page immediately shows correct thumbnail state, screenshot filter works, no re-harvest needed. Half-day implementation.

If rejected: we re-harvest to backfill, fix the thumbnail path fallback in the Turtle parser, and accept the harvest-to-see-changes cycle.
