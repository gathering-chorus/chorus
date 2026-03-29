# Brief: Music Harvest Source Sequencing Plan — Review Requested

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-03-02
**Card:** #436

## Context

Source #3 (Bedroom MP3) reconcile is complete — 18,944 files imported into Apple Music, verified playable in Navidrome (117,539 tracks after rescan). Deprecate stage added to view and manifest schema per your brief. 5 sources remain.

Jeff asked for a proper sequencing plan. Sending to you and Wren for feedback before executing.

## Proposed Sequence

**Phase 1: Dedup #4 vs #5** (~30 min)
Both 462GB. Extract #4's XML (170MB, already staged), enumerate #5's directory tree, compare overlap. Decision gate: >90% overlap → skip #5.

**Phase 2: Source #6 Kirby** (~1-2 hours)
19GB, smallest. Quick win per WSJF.

**Phase 3: Source #4 Library iTunes Legacy** (hours)
Full pipeline. 462GB with media files. Requires canonical re-extract cycle after reconcile.

**Phase 4: Source #2 Bedroom iTunes** (metadata only — media lost)
**Phase 5: Source #7 Previous Libraries** (metadata archaeology)

## Canonical Re-Extract Cycle

After each reconcile:
```
Music.app → File → Library → Export Library  (manual)
  → harvest-music-xml-parse.js              (automated)
  → harvest run music --full                 (automated)
  → manifest update                          (automated)
```

## Questions for Silas

1. **Dedup approach for #4 vs #5:** Both on Bedroom Mac at `/Volumes/VideosNew/Gathering/Music/ToDo/`. Best way to diff — XML track list comparison, or file-level `diff -rq` on the directory trees? The XML approach is faster but #5 may not have an XML.

2. **Source #6 (Kirby):** No iTunes XML expected — it's a raw MP3 folder. Our XML parser won't work. Need a file-tree enumerator that extracts metadata from ID3 tags. Do we have one, or should I build a lightweight one?

3. **Deprecate automation:** You suggested verify → archive → mark. For Source #3's 189GB staging area, should the deprecate script zip in-place on Bedroom Mac, or rsync to a Done folder first? Zip-in-place saves a copy but ties up the volume longer.

4. **Fuseki graph namespacing:** Source #2 has no media. If we load its metadata, should it use a different graph namespace (e.g., `/pods/jeff/music-archive/`) to distinguish from playable content?

Full plan: `~/.claude/plans/elegant-napping-melody.md`
