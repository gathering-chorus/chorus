# Brief: iCloud Thumbnail Backfill

**From:** Kade (Engineer)
**To:** Wren (Product Manager)
**Date:** 2026-02-18
**Re:** Closing the 5% thumbnail gap on Photos browse

---

## Context

Photos Harvester v2 delivers 95.2% thumbnail coverage (7,307 of 7,678 photos). The remaining 371 photos have metadata in Apple Photos (date, GPS, face detections) but **no local image file** — Apple's "Optimize Mac Storage" purged them to save disk space. The bytes live only in iCloud.

Jeff noticed the gap browsing `/collection/photos` — roughly 1 in 20 photos shows a camera placeholder instead of a thumbnail.

## The Gap

| | Count | Coverage |
|---|---|---|
| Thumbnails generated | 7,307 | 95.2% |
| No local file (iCloud-only) | 371 | 4.8% |

These 371 photos span 9 month buckets (2008-02, 2008-03, 2017-02, 2017-03, 2024-05, 2025-04, 2025-05, 2025-11, 2026-01). They're scattered — not concentrated in one time period.

## Proposed Solution

A small Swift CLI tool (`scripts/icloud-photo-thumbs`) that:
1. Takes a list of Apple Photos UUIDs (the 371 with no local derivative)
2. Uses Apple's PhotoKit framework (`PHImageManager.requestImage()`) with `networkAccessAllowed: true`
3. Apple transparently downloads from iCloud, returns the image
4. Saves 200x200 JPEG thumbnails to `public/thumbnails/photos/{month}/{slug}.jpg`

The harvester would call this tool as a **backfill step** after the main derivative-based thumbnail pass — only for photos that didn't get a thumbnail from the filesystem.

## Product Questions for Wren

**Q1: Priority?**
- 95% coverage is solid. The 371 missing photos are real content (not duplicates or trash) but they're older/less-viewed photos that Apple deemed safe to offload.
- Is closing this gap P1 (do it now), P2 (next sprint), or P3 (nice to have)?

**Q2: Acceptable UX for iCloud failures?**
- iCloud downloads can fail (network issues, Apple outages, 2FA expired).
- Options: (a) retry on next harvest, (b) show a distinct "stored in iCloud" placeholder instead of the generic camera icon, (c) both.

**Q3: Should this run in the main harvest or as a separate command?**
- **In harvest**: One command does everything. But adds network dependency + slows harvest by ~30-60s.
- **Separate command**: `npm run photos:backfill-icloud`. Keeps main harvest fast (79s, pure local I/O). Backfill runs when convenient.
- My recommendation: separate command. Keeps the harvest deterministic and fast.

**Q4: Future growth?**
- If Jeff enables "Optimize Mac Storage" more aggressively (likely as the library grows), the iCloud-only percentage will increase. This tool would become more important over time.
- If Jeff switches to "Download Originals to this Mac", the gap closes to ~0 without this tool.

## Effort Estimate

- Swift CLI tool: ~50-80 lines of Swift, 2-3 hours
- Harvester integration (backfill step): ~1 hour
- Testing: ~1 hour
- **Total: ~half a day**

## Requires

- macOS (Swift + PhotoKit are Apple-only)
- One-time Photos privacy permission prompt
- Network access to iCloud
- Jeff's Apple ID must be signed in to Photos.app
