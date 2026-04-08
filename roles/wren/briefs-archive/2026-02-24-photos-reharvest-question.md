# Photos Re-Harvest Question

**From**: Kade (Engineer) → Wren (PM)
**Re**: #311 — Do we need to re-harvest photos?
**Date**: 2026-02-24

## Context

I just built source path tracking into both harvesters (compiles clean, 2310 tests pass). The photos harvester now extracts:
- `jb:sourceFilePath` — absolute path to the original file
- `jb:sourceFileHash` — Apple Photos content fingerprint
- `jb:originalFileSize` — file size in bytes

These fields didn't exist when the current photos were harvested. That means **every photo already in RDF is missing these three fields.**

## The Question

Do we need to re-harvest photos to backfill source paths, hashes, and file sizes? Or do we wait and harvest fresh from /Gathering once the copy completes?

## What I Know About Photo Locations

### Primary Mac (local)

| Location | Size | File Count | What It Is |
|----------|------|------------|------------|
| `~/Pictures/Photos Library.photoslibrary` | present | ~44,193 files | **Active Apple Photos library** — this is what the harvester reads (SQLite). Contains originals + derivatives. |
| `~/Downloads/Pictures` | 44 GB | ~18,796 files (16,621 jpg, 236 heic, 210 png) | Loose files — export or backup. NOT an Apple Photos library. Harvester can't read these (no SQLite). |

### Secondary Mac (SMB at /Volumes/Gathering)

| Location | Size | What It Is |
|----------|------|------------|
| `/Volumes/Gathering/Pictures` | 16 KB | **Empty placeholder** — nothing there yet |

**Jeff is currently copying `~/Downloads/Pictures` → `/Volumes/Gathering`** (~24h). After that, the secondary Mac will have ~44GB of loose photo files.

## The Gap

The photos harvester only reads from **Apple Photos SQLite** (`~/Pictures/Photos Library.photoslibrary`). It can't harvest loose files in `~/Downloads/Pictures` or `/Volumes/Gathering/Pictures`. Those ~18,796 loose files are dark to the current harvester.

**Two separate problems:**
1. **Re-harvest the Apple Photos library** to backfill source paths/hashes/sizes on existing photos → straightforward, just re-run the harvester
2. **Harvest loose photo files** from Downloads/Gathering → needs a new "file-based" photo ingestion path (doesn't exist yet)

## My Recommendation

1. **Re-harvest Apple Photos now** — the library is local, harvester is ready, gets us source paths on the ~44K photos already indexed
2. **Don't block on loose files** — that's a new harvester capability (scan directory for jpg/heic/png, extract EXIF, ingest). Card it separately.
3. **Wait for music harvest** until /Gathering copy completes — music harvester reads from Apple Music app, not files on disk

What's your call?
