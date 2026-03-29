# Brief: Music Harvest Source Sequencing Plan — Review Requested

**From:** Kade (Engineer)
**To:** Wren (PM)
**Date:** 2026-03-02
**Card:** #436

## Context

Source #3 (Bedroom MP3) reconcile is complete — 18,944 files imported into Apple Music, verified playable in Navidrome. 5 sources remain. Jeff asked for a proper sequencing plan before we proceed.

## Current State

| # | Source | Status | Size | Notes |
|---|--------|--------|------|-------|
| 1 | library-mac (canonical) | **All stages complete** | 867GB | 87,367 tracks. Baseline. |
| 2 | bedroom-itunes | Not started | ? | **Media lost** — 98K tracks in XML but drive failed. |
| 3 | bedroom-photosnew | **Complete** | 189GB | 18,944 imported, Navidrome verified. Deprecate pending. |
| 4 | library-itunes-legacy | Not started | 462GB | 170MB XML available. Blocked on #5 dedup. |
| 5 | library-music-snapshot | Not started | 462GB | Same size as #4 — likely duplicate. |
| 6 | bedroom-kirby | Not started | 19GB | 2,472 items from 2010 PC backup. Untriaged. |
| 7 | library-previous-libraries | Not started | 0GB | 5 .musiclibrary files (2021-2025). Metadata only. |

## Proposed Sequence

**Phase 1: Dedup #4 vs #5** (~30 min)
Both 462GB, same size. Extract #4's XML, enumerate #5's directory tree, compare. If >90% overlap, skip #5. This is the highest-value decision — saves 462GB of redundant work.

**Phase 2: Source #6 Kirby** (~1-2 hours)
Smallest untriaged source (19GB). WSJF tiebreaker — smallest first. Quick feedback on unique content.

**Phase 3: Source #4 Library iTunes Legacy** (hours)
Largest remaining source with media. Full pipeline: extract → transform → reconcile → load. Requires re-export of canonical Apple Music XML afterward.

**Phase 4: Source #2 Bedroom iTunes** (metadata catalog)
Media is gone. Catalog 98K track metadata into Fuseki as historical record? **Decision needed: worth it or skip?**

**Phase 5: Source #7 Previous Libraries** (metadata archaeology)
Parse .musiclibrary files to find deleted tracks. Report only. **Decision needed: worth the effort?**

## Key Pattern

After any reconcile that imports tracks, the canonical must cycle:
```
Music.app export XML → re-extract → re-transform → re-load Fuseki
```
The XML export is manual (Jeff does it). Everything else is automated.

## Questions for Wren

1. **Source #2 (lost media):** Is cataloging 98K orphaned metadata records a product priority? Or skip it?
2. **Source #7 (library archaeology):** Is discovering historically deleted tracks worth engineering time?
3. **Demo pattern:** We proved reconcile with a Navidrome playback test (search imported artist, play track). Should this be the standard acceptance test for each source?
4. **Does the phase ordering make sense from a product perspective?**

Full plan: `~/.claude/plans/elegant-napping-melody.md`
