# Canonical Photo Serving Model

**Card:** #1504
**Author:** Silas
**Date:** 2026-03-21
**Status:** Approved — Wren reviewed 2026-03-21

## Problem

Photos has 4 storage locations, 3 sources, and no unified serving path. The /photos page shows wrong dates (harvest date, not EXIF), lightbox shows thumbnails instead of originals, and the handler has source-specific branching. Jeff flagged all of these directly.

## Storage Map

| Source | Originals | Thumbnails | Location | Access |
|--------|-----------|------------|----------|--------|
| Apple Photos | `~/Pictures/Photos Library.photoslibrary/originals/{0-F}/UUID.{heic,jpeg}` | `resources/derivatives/{0-F}/UUID_*.jpeg` | Library Mac | Local filesystem |
| Google Takeout | `/Volumes/Gathering/Photos/GoogleTakeoutPhotos/extracted/Takeout/Google Photos/...` | `/Volumes/Gathering/Photos/GoogleTakeoutThumbnails/` | Bedroom Mac (NFS) | NFS mount |
| Google Drive | `/Volumes/Gathering/Photos/` (routed subset) | None generated | Bedroom Mac (NFS) | NFS mount |
| osxphotos | Same as Apple Photos (supplemental metadata only) | N/A | Library Mac | N/A (metadata source) |

**Key index:** `takeout-fullres-index.json` — 129,580 entries mapping content hash → NFS path to Google Takeout full-res files.

## Design: One URI, One Code Path

### Resolution Chain

Given a canonical photo URI (e.g., `urn:jb:photos/canonical/2019-09/img_4272`):

```
1. Look up canonical node in Fuseki → get hasSourceRecord URIs
2. Determine source priority: Apple > Google Takeout > Google Drive
3. For the best source, resolve to filesystem path
4. Serve with fallback chain: original → derivative → thumbnail → placeholder
```

### Source → Path Resolution

**Apple Photos:**
```
Source URI: urn:jb:photos/items/YYYY-MM/{slug}
→ Extract UUID from Apple Photos SQLite (by filename match)
→ Path: ~/Pictures/Photos Library.photoslibrary/originals/{uuid[0]}/{UUID}.{ext}
```

**Google Takeout:**
```
Source URI: urn:jb:photos/google/{slug}
→ Lookup in takeout-fullres-index.json (by filename or content hash)
→ Path: /Volumes/Gathering/Photos/GoogleTakeoutPhotos/extracted/...
→ CONSTRAINT: NFS read, not bulk copy (DEC-089)
```

**Google Drive:**
```
Source URI: urn:jb:photos/albums/import-photosnew.ttl items
→ Same NFS path resolution as Takeout
```

### API: Single Endpoint

```
GET /api/photos/serve/:canonicalId
```

Resolution:
1. Query canonical graph for source records
2. Try Apple original (local, fast)
3. Try Google Takeout full-res (NFS, slower)
4. Fall back to derivative/thumbnail
5. Return 404 with metadata if no file found

Response headers include `X-Photo-Source` (apple-photos, google-takeout) and `X-Photo-Resolution` (original, derivative, thumbnail) for debugging.

### Date Fix

**Rule:** Date is always EXIF `DateTimeOriginal`, never harvest/export date.

Current state:
- Apple records: `jb:dateTaken` from osxphotos = correct (EXIF)
- Google records: `dcterms:created` = Google export timestamp = **wrong**

Fix: Canonical graph already prefers Apple's `dateTaken` for filename-match records. For Google-only records, the Takeout JSON sidecar files contain the original photo date. Backfill script reads sidecar JSON and updates canonical `dateTaken`.

### Thumbnail Generation

Current: 0 thumbnails locally. Apple derivatives exist in Photos Library.

Plan: Generate thumbnails for all 62,998 canonical photos on Bedroom Mac (has the CPU + storage). Store at `data/pods/jeff/photos/thumbnails/{YYYY-MM}/{slug}.jpg` at 400px wide. Run as batch job via SSH to Bedroom.

## Implementation Plan (for Kade)

1. **New service:** `photo-resolver.service.ts` — single `resolve(canonicalUri): { path, source, resolution }` method
2. **New endpoint:** `GET /api/photos/serve/:id` — proxy through resolver
3. **Refactor handler:** Remove source-specific branching from `serveDerivative`, delegate to resolver
4. **Backfill dates:** Script to read Google Takeout JSON sidecars and update canonical graph
5. **Thumbnail batch:** SSH script for Bedroom, generates 400px JPGs

## Constraints

- C1: NFS for individual file reads only, not bulk scans (DEC-089)
- C2: Apple Photos Library path is machine-specific — no hardcoded paths in service
- C3: Thumbnail generation runs on Bedroom, not Library (CPU + proximity to files)
- C4: No new Docker containers — native service or LaunchAgent

## Decisions (Wren, 2026-03-21)

1. **Stream on-demand from NFS**, don't pre-cache to Library. Cache thumbnails only. Disk at 87%, can't afford 63K photo copies.
2. **Pre-convert HEIC→JPEG as a Bedroom batch job.** Don't burn Library CPU on serve-time conversion. Bedroom has the M2 Pro for this.

**Status:** Design approved. Ready for Kade to implement.
