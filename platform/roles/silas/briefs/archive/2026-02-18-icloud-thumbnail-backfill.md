# Brief: iCloud Thumbnail Backfill — Architecture Review

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-02-18
**Re:** Proposed Swift CLI for iCloud photo thumbnail retrieval

---

## Context

Photos Harvester v2 generates thumbnails from Apple Photos filesystem derivatives (cached resized JPEGs at `~/Pictures/Photos Library.photoslibrary/resources/derivatives/`). This covers 95.2% of photos. The remaining 371 photos (4.8%) have no local file — Apple's "Optimize Mac Storage" purged them. Metadata exists in SQLite, image bytes only in iCloud.

## Current Architecture

```
SQLite DB ──→ PhotoSqliteService ──→ metadata (all 7,678 photos)
Derivatives ──→ sharp resize ──→ thumbnails (7,307 photos)
                                    └── 371 gap (no local file)
```

**Derivative patterns checked:** `_1_105_c.jpeg` (8,174 hits), `_1_102_o.jpeg` (765), `_1_102_a.jpeg` (15, not currently checked). `.THM` files exist (1,381) but are 32x32 — unusable.

## Proposed Addition

A compiled Swift CLI tool that uses Apple's PhotoKit framework to request thumbnails with iCloud download enabled:

```
scripts/icloud-photo-thumbs.swift
  ├── Input: JSON list of { uuid, dateBucket, slug }
  ├── Uses PHAsset.fetchAssets(withLocalIdentifiers:)
  ├── Uses PHImageManager.requestImage() with:
  │     targetSize: 200x200
  │     contentMode: .aspectFill
  │     networkAccessAllowed: true  ← triggers iCloud download
  └── Output: JPEG files at public/thumbnails/photos/{bucket}/{slug}.jpg
```

**Integration point:** Called from `PhotoHarvesterService.harvest()` after the derivative thumbnail pass, only for UUIDs that didn't get a thumbnail. OR as a standalone `npm run photos:backfill-icloud` command.

## Architecture Questions for Silas

**Q1: Swift CLI as a build dependency — acceptable?**
- This introduces a compiled native binary into the Node.js project.
- Compile step: `swiftc scripts/icloud-photo-thumbs.swift -o scripts/icloud-photo-thumbs` (one-time, ships binary OR compiles on first run).
- Alternative: Python via PyObjC (avoids compiled binary but adds Python dependency).
- Alternative: JXA calling Photos framework (unreliable for bulk operations, as we learned).
- My recommendation: Swift CLI. It's the canonical way to use PhotoKit. macOS-only is already a constraint (Apple Photos is macOS-only). Single file, no external dependencies.

**Q2: Does this break Pattern B?**
- Pattern B: extract → normalize → write Turtle.
- Current thumbnail generation is a side-effect of the harvest (writes files to `public/thumbnails/` during the extract phase).
- Adding iCloud backfill is the same pattern — just a different extraction source for the same output.
- I don't think this breaks Pattern B, but want your read.

**Q3: Error handling for network-dependent extraction?**
- SQLite + filesystem extraction is deterministic — same input, same output, no network.
- iCloud extraction introduces network failure modes: timeout, auth expired, partial download.
- Proposal: treat iCloud thumbnails as best-effort. Log failures, don't fail the harvest. Retry on next run.
- The Turtle metadata is already complete (written from SQLite). Only the thumbnail file is affected.

**Q4: Should this be a new service or extend PhotoSqliteService?**
- Option A: New `PhotoIcloudService` — clean separation, different failure modes.
- Option B: Add method to `PhotoSqliteService` — it already handles derivatives, this is "another thumbnail source."
- Option C: Keep it external — the Swift CLI is a standalone tool, harvester just shells out to it.
- My recommendation: Option C. The Swift CLI is a black box. Harvester passes UUIDs, gets thumbnails. No new TypeScript service needed.

**Q5: Disk impact?**
- 371 additional thumbnails at ~15KB each = ~5.5MB. Negligible.
- The iCloud download is temporary — PhotoKit returns the image in memory, we resize + save, no full-resolution file cached.

## Implementation Plan

1. Write `scripts/icloud-photo-thumbs.swift` (~50-80 lines)
2. Add compile step to package.json or Makefile
3. Add backfill step in harvester (or standalone script)
4. Handle: privacy permission prompt, network errors, partial completion
5. Test with a small batch (10 UUIDs) before running full 371

## Risk

- **Privacy permission**: First run triggers macOS "Allow access to Photos?" dialog. One-time, but requires Jeff's click.
- **Apple ID auth**: If Jeff's iCloud session expires, PhotoKit may fail silently or return placeholders.
- **Future macOS changes**: PhotoKit API is stable but Apple could change behavior.
- **CI/CD**: This won't work in GitHub Actions (no Photos library, no iCloud). Tests must mock or skip.
