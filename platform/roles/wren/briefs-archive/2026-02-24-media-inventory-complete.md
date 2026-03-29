# Media Inventory Complete — Full Two-Mac Scan

**From**: Kade (Engineer) → Wren (PM)
**Re**: #311 — Media rescue, full findings
**Date**: 2026-02-24

## What Changed

Scanned the secondary Mac via SSH (`Jeffs-Mac-mini.local`). The SMB mount only showed one shared folder. The full picture is significantly larger.

## Key Discoveries

### 175K Photos on External Storage
`/Volumes/PhotosNew/Pictures/` on the secondary Mac has **175,210 files** in date-bucketed folders spanning 2007–2018. This is likely Jeff's original photo archive predating Apple Photos. Completely dark to the current harvester. There's also another Apple Photos library and a Picasa library on the same drive.

### Additional Music Collections
`/Volumes/PhotosNew/mp3/` has:
- Another iTunes library with **28,464 m4a** files (mid-sized, likely intermediate state)
- An older **"iTunes Music" directory with 15,686 mp3s** — separate collection, may contain tracks never migrated to Apple Music

### 150 TB Video Library
22 external drives, ~150 TB used, organized alphabetically by performer. No harvester exists. Not in current scope but worth knowing the scale.

### Secondary Mac is a Storage Box
Its own Music library has 1 track (Lionrock). Its own Photos library has 61 originals. All the content is on external drives.

## Full Inventory

Written to `/tmp/media-source-inventory.md` (opened in Chrome for Jeff). Covers every music, photo, and video source across both Macs with file counts, sizes, harvester compatibility, and my assessment.

## The Scale Problem

| Media | Known (Primary Mac) | Discovered (Secondary Mac) | Total |
|-------|-------------------|---------------------------|-------|
| Music | ~98K tracks + 2 copies | 28K m4a + 16K mp3 on external | ~100K+ unique potential |
| Photos | 44K (Apple Photos) + 19K loose | **175K on external** + Photos library + Picasa | **~200K+ potential** |
| Video | — | ~150 TB across 22 drives | massive |

## What Jeff Said

"I need to think about how we proceed." This is the right call — the scope expanded significantly from "rescue music library" to "inventory reveals 175K dark photos and 44K additional music files across external storage."

## What's Ready (Code)

Source path tracking is built and tested for both harvesters (tsc clean, 2310 tests pass). Ready to commit whenever Jeff decides on harvest strategy. Changes:
- Music: JXA `t.location()` → `jb:sourceFilePath` in RDF
- Photos: `ZDIRECTORY` + `ZADJUSTEDFINGERPRINT` + `ZORIGINALFILESIZE` → `jb:sourceFilePath`, `jb:sourceFileHash`, `jb:originalFileSize`

## What's NOT Ready

- File-scan harvester for loose photos (EXIF-based ingest from directories)
- iTunes XML parser for legacy tracks
- Remote harvesting (can SSH but can't run JXA or SQLite remotely)
- Video harvester
- Cross-library dedup tooling

## My Recommendation

Wait for Jeff's direction. The decisions here are product decisions (what to rescue first, what "rescued" means, priority order) not engineering ones. The code changes are ready whenever he decides to move.
