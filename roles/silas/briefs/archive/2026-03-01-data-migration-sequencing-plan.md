# Data Migration Sequencing Plan

**Author:** Silas (Architect)
**Date:** 2026-03-01
**Status:** Draft — pending Kade input
**Context:** Jeff asked for a sequenced, repeatable approach to data migrations. Current state was validated tonight by Silas against live systems. Several claims from prior sessions were incorrect.

---

## Problem

Data migrations (music, photos, media consolidation) have been running ad hoc — multiple rsyncs kicked off in parallel, some over SMB (which fails), some local (which works), no clear sequencing, no verification gates between steps. Tonight we found:

- Redundant rsyncs running (killed them)
- A completed serial rsync that Kade thought was still in progress
- Artwork numbers that were wrong (1,746 missing, not 2,965)
- No verification step between "rsync done" and "next pipeline stage"

We need a sequenced plan where each step has a **precondition**, an **action**, a **verification**, and a **manifest update**.

---

## The Pattern: Gather → Match → Deprecate

Every source follows the same lifecycle:

```
Original Source → rsync to Gathering/ToDo/ → process into pipeline → verify 100% match → deprecate original
```

1. **Gather:** rsync source into `/Volumes/Gathering/<domain>/ToDo/<source-name>/` (always local on Bedroom, never over SMB)
2. **Process:** Extract → Transform → Load into Fuseki via harvest pipeline
3. **Match:** Diff the Gathering copy against the original source — 100% file-level match required
4. **Deprecate:** Only after verified match. Original source can be unmounted/archived/deleted. Not before.

The `ToDo/` folders are staging areas. Files move to `Done/` after matching. The Gathering volume becomes the single canonical copy. This is how Jeff wants every domain to work.

**Current Gathering structure on Bedroom (`/Volumes/VideosNew/Gathering/`):**
```
Music/
  ToDo/
    Bedroom-mp3/        (rsync complete — 75GB)
    Library-iTunes/     (existing snapshot)
    Library-Music-Snapshot/
  Done/                 (empty)
Photos/
  ToDo/
    Bedroom-iPhone/     (rsync complete — 307 files)
    Bedroom-PhotosNew/  (rsync complete — 605 dirs)
    Bedroom-iPhoto/     (rsync complete — 71GB)
  Done/                 (empty)
```

---

## Verified Current State (2026-03-01 19:00 Boston)

### Music (#436)

| Fact | Value | How verified |
|------|-------|-------------|
| Albums in Fuseki | 23,709 | SPARQL COUNT query |
| TTL directories on disk | 6 (files nested inside) | `ls` |
| Source #3 import | Complete — 18,944 total | `/tmp/music-import-remote.log` says "Import Complete" |
| Source #3 failures | 150 | Import log |
| Artwork: albums WITH coverArt | 8,907 | `grep -rl coverArt` on TTL files |
| Artwork: albums WITHOUT coverArt | 1,746 | `grep -rL coverArt` on TTL files |
| Artwork backfill process | Not running (last ran Feb 28) | No process found |
| Source #2 (Gathering/iTunes) | Deferred — volume not mounted | Manifest |

### Photos (#437)

| Fact | Value | How verified |
|------|-------|-------------|
| TTL files on disk | 7 directories | `ls` |
| Graphs in Fuseki | 633 | SPARQL COUNT query |
| Verify stage | Never done | Manifest |
| Bedroom sources in pipeline | No | Not started |

### Media Migration (rsync to /Volumes/Gathering)

| Fact | Value | How verified |
|------|-------|-------------|
| Serial rsync (Bedroom local) | ALL DONE — 3/3 steps exit 0, 148GB | `/tmp/rsync-serial.log` |
| iPhone rsync | Complete (308 files) | Destination dir: 307 files |
| PhotosNew rsync | Complete | Destination dir: 605 dirs |
| iPhoto rsync | Complete (71GB) | Destination dir: 63 dirs + log confirms exit 0 |
| SMB rsyncs | Killed (were redundant) | `ps aux` confirms no rsync processes |

---

## Sequenced Plan

### Phase 1: Complete Music Pipeline (estimated: 1 session)

**Precondition:** Source #3 import is complete (verified). Serial rsync done.

#### Step 1.1: Re-export Music XML
- **Who:** Jeff (manual — Music.app → File → Library → Export Library → `/tmp/music-library-export.xml`)
- **Verify:** `test -f /tmp/music-library-export.xml && stat -f '%z' /tmp/music-library-export.xml`
- **Expected:** File exists, size > 100MB (was 87K tracks before, now should be ~104K)

#### Step 1.2: Re-run extract
- **Command:** `harvest run music extract`
- **Verify:** `jq '.stages.extract.output_count' data/harvest/manifests/music.json` — expect > 100,000 (was 87,386 before Source #3 import)
- **Manifest update:** Automatic (harvest CLI updates manifest)

#### Step 1.3: Re-run transform + load
- **Command:** `harvest run music transform` (load is bundled)
- **Verify:** SPARQL count of music graphs > 23,709 (current)
- **Verify:** Spot check — pick 3 newly imported artists, confirm they appear in album browse
- **Manifest update:** Automatic

#### Step 1.4: Artwork backfill (remaining 1,746)
- **Command:** `npx ts-node scripts/backfill-artwork.ts` (needs to be restarted)
- **Verify:** `grep -rL coverArt data/pods/jeff/music/albums/ | wc -l` — should decrease from 1,746
- **Rate limit:** ~20 req/min, ~1.5 hours for 1,746 albums at 91% hit rate
- **Manifest update:** Update `tasks[artwork-backfill].count_done` when complete

#### Step 1.5: Investigate 150 failed Source #3 imports
- **Command:** `grep -i fail /tmp/music-import-remote.log | head -20`
- **Decision:** Are these recoverable? If <5% of unique tracks, accept and document.
- **Manifest update:** Add `source3_result.import_failed_investigated: true`

#### Step 1.6: Verify end-to-end
- **Checklist (human — Jeff or Kade):**
  - [ ] Album browse renders with cover art
  - [ ] Search returns music results for a Source #3 artist
  - [ ] Track count in Fuseki matches new extraction count
  - [ ] Navidrome links resolve for a newly imported album
- **Command:** `harvest run music verify` (runs the checklist script)
- **Manifest update:** Set verify.status = complete

**Gate:** Music is Done when Step 1.6 passes. Move #436 scope card.

---

### Phase 2: Photos Pipeline Maturity (estimated: 2 sessions)

**Precondition:** Music Phase 1 complete. Jeff has bandwidth.

Jeff blocked photos because "too manual/hacky." The fix is to make photos pipeline as scriptable as music before running it on Bedroom sources.

#### Step 2.1: Verify existing Library photos (633 Fuseki graphs)
- **Command:** `harvest run photos verify`
- **Checklist:**
  - [ ] Photo browse page renders
  - [ ] Search returns photo results
  - [ ] Thumbnails display (or identify that thumbnail pipeline doesn't exist yet)
- **Manifest update:** Set verify.status based on results
- **Decision point:** If verify fails, fix before adding Bedroom sources

#### Step 2.2: Script the photos extract → transform → load pipeline
- **Current state:** Extract uses SQLite direct read (replaced JXA). Transform produces TTL. Load syncs to Fuseki.
- **Goal:** `harvest run photos --full` works end-to-end without manual intervention
- **Kade task:** Review existing scripts, identify manual steps, automate them
- **Verify:** Run `harvest run photos extract` on Library source, confirm manifest updates automatically

#### Step 2.3: Inventory and diff Bedroom photo sources
- **What's in Gathering/Photos/ToDo/ (all rsyncs complete):**
  - `Bedroom-iPhone/` — 307 files
  - `Bedroom-PhotosNew/` — 605 dirs
  - `Bedroom-iPhoto/` — 71GB
- **Need:** Count unique photos across all 3 sources, identify duplicates vs Library and vs each other
- **Command:** Write a `photos-source-diff.js` (like `music-source-diff.js`) that diffs ToDo sources against Library
- **Output:** Report per source: total files, unique count, overlap %, estimated processing time

#### Step 2.4: Process each source through pipeline (one at a time)
- **Precondition:** Step 2.2 pipeline is scripted, Step 2.3 diff is done
- **Approach:** Process one source at a time (iPhone first — smallest)
- **Sequence:** iPhone (307) → PhotosNew (605 dirs) → iPhoto (71GB)
- **Per source:**
  1. Extract from Gathering/Photos/ToDo/<source>
  2. Transform → TTL
  3. Load → Fuseki
  4. Verify: Fuseki count increases, browse page shows new photos
  5. Match: diff Gathering/Photos/ToDo/<source> against original on source volume — must be 100%
  6. Move matched source from `ToDo/` to `Done/`
- **Manifest update:** After each source, update photos manifest with new counts

#### Step 2.5: Deprecate originals
- **Precondition:** ALL sources in `Done/`, 100% match verified for each
- **Action:** Jeff decides when/if to unmount or archive original source volumes
- **This is Jeff's call, not ours.** We provide the match report; he decides when to let go.

**Gate:** Photos Done when all sources in `Done/` and verified.

---

### Phase 3: Remaining Domains (estimated: 1 session each, lower priority)

#### 3.1: Notes — run verify, sync 823 unloaded TTL
- **Command:** `harvest run notes load` then `harvest run notes verify`

#### 3.2: Stories — sync remaining 48 TTL to Fuseki
- **Command:** Manual load (stories pipeline is manual by design)

#### 3.3: Facebook/LinkedIn — blocked on archive exports (external dependency)

#### 3.4: Sexuality — blocked on 9/29 offline volumes (hardware dependency)

---

## Operating Rules (apply to all phases)

1. **One domain at a time.** Don't run music and photos pipelines simultaneously. Fuseki contention and disk I/O compound.

2. **Local rsync only.** Never rsync over SMB from Library to Bedroom. If data needs to move to Bedroom, SSH to Bedroom and run rsync locally. Tonight's failure confirmed this.

3. **Gather → Match → Deprecate.** Every source follows the same lifecycle. Files go to `Gathering/ToDo/`, get processed through the pipeline, get diffed against the original source for 100% match, then move to `Done/`. Original sources are only deprecated after verified match. No exceptions.

4. **Verify before next step.** Every stage has a verify action. Don't start transform until extract is verified. Don't start the next domain until the current one passes its gate.

5. **Manifest is truth.** After every stage, check that the manifest updated. If it didn't, the stage didn't actually run. `harvest sync-board` after each domain completes.

6. **One process per domain.** No parallel rsyncs for the same data. Serial is safer and actually faster (avoids I/O contention on external USB drives).

7. **Jeff exports, Kade processes.** XML exports from Music.app and Photos.app require Jeff. Everything after export is scriptable by Kade.

8. **Nothing gets deleted until matched.** The whole point of Gathering is consolidation, not destruction. Deprecation means the original is no longer the canonical copy — it doesn't mean it's gone. Jeff decides when originals are removed.

---

## Monitoring (shipped tonight, #653)

- `harvest` CLI shows pipeline status for all domains
- `harvest sync-board` updates scope cards with manifest state
- Boot hook shows one-line harvest summary at session start
- Prometheus alerts fire if any stage is stale >48h
- Grafana Data Center dashboard has capacity context

---

## Open Questions for Kade

1. **Photos extract:** Is the SQLite-based extract fully scripted, or does it require manual steps? Can `harvest run photos extract` run end-to-end?
2. **Artwork backfill:** The script needs restarting — does it pick up where it left off, or does it re-process all 4,965?
3. **Photos thumbnails:** Is there a thumbnail generation step? The music pipeline has cover art; photos needs something similar for browse.
4. **150 failed music imports:** Have you looked at the failures? Are they format issues, missing files, or Apple Music API errors?
5. **Photos source diff:** Does `music-source-diff.js` have a pattern we can reuse for photos, or is the data shape too different?
