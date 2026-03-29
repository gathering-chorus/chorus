# Brief: Music Harvest — Immediate Actions (no waiting)

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02 06:55 Boston
**Priority:** P1 — start now, don't wait for XML export
**Card:** #436 (Music harvest scope)

## Context

Jeff wants us flowing on data harvests. Music is 90% done — smallest remaining work. Two steps can start RIGHT NOW, independent of Jeff's XML re-export.

## Do Now (no dependencies)

### 1. Artwork Backfill (1,746 albums missing coverArt)

The backfill script needs restarting. Key question: does it resume from where it left off, or does it re-scan all albums?

```bash
# Check the script
cat scripts/backfill-artwork.ts | head -50

# If it resumes:
npx ts-node scripts/backfill-artwork.ts

# Monitor progress
grep -rL coverArt data/pods/jeff/music/albums/ | wc -l
# Should decrease from 1,746
```

Rate limit: ~20 req/min, ~1.5 hours at 91% hit rate. Can run in background.

### 2. Investigate 150 Failed Music Imports

Source #3 import completed with 150 failures. Determine if they're recoverable.

```bash
grep -i fail /tmp/music-import-remote.log | head -20
# Are these format issues, missing files, or API errors?
# If <5% of unique tracks, document and accept
```

**Output needed:** Brief back to `architect/briefs/` with:
- Backfill: does it resume? How many remain after running?
- Failures: what type? Recoverable? Accept or retry?

## Do After Jeff (needs XML export)

Steps 1.1–1.3 from the migration plan need Jeff to re-export from Music.app first. We'll coordinate that separately.

## Migration Plan

Full plan: `architect/briefs/2026-03-01-data-migration-sequencing-plan.md`
Review brief (with 5 questions): `engineer/briefs/2026-03-02-data-migration-plan-review.md`

Answer those 5 questions when you can — but don't block on them. Start the artwork backfill now.
