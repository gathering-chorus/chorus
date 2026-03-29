# Brief: Music & Photo File Scan — Orphans, Stranded Data, and Harvest Inputs

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-06
**Priority:** P1 — Jeff and Kade working on music/photo harvests today
**Context:** NFS mount restored (`com.gathering.nfs-mount` LaunchDaemon installed for persistence). Full scan of both machines completed.

---

## TL;DR

Scanned both Library and Bedroom for orphaned/stranded music and music library files. Found 9.2GB of confirmed dupes on Library, legacy iTunes metadata with play history worth harvesting, a mystery "Kirby" collection, and 4GB of Outlook PSTs hiding in the music ToDo folder.

---

## 1. NFS Mount Status

- **Fixed today.** Mount did not survive last reboot — no persistence was configured.
- **Now:** `192.168.86.242:/Volumes/VideosNew/Gathering` -> `/Volumes/Gathering` (NFS)
- **Persistence:** LaunchDaemon `com.gathering.nfs-mount` installed at `/Library/LaunchDaemons/`
- **Canonical music path:** `/Volumes/Gathering/Music/Music/` — **252,232 audio files**, 2,790 artist dirs, **809GB**
- **DRM WARNING:** 34,606 files are `.m4p` (FairPlay DRM) — old iTunes Store purchases. These will fail silently in any harvest/transcode pipeline. Filter by extension or detect with `file` command.

---

## 2. Library Mac (192.168.86.36) — Findings

### jeff-music local duplicates — 9.2 GB (CLEANUP TARGET)

- **Path:** `~/Music/jeff-music/Media.localized/Music/`
- **Files:** 1,753 audio files (1,301 m4a + ~450 mp3), 120 artist dirs
- **Size:** 9.2 GB
- **All 120 artist dirs confirmed present in canonical NFS path** — these are duplicates Apple Music consolidated locally.
- **Action needed:** Verify which Apple Music library is active before deleting:
  - `~/Music/Music/Music Library.musiclibrary` — default library, 35MB database, empty media dir
  - `~/Music/jeff-music/Music Library.musiclibrary` — 1MB database, has the 9.2GB local media
  - Check Apple Music preferences or Option+launch to confirm which is selected
  - If jeff-music is the active library and it references NFS paths, the local copies under `Media.localized/Music/` are safe to delete — Apple Music will read from NFS instead
  - If jeff-music references local paths (not NFS), deleting breaks the library — would need to repoint first

### Original source — CLEAN

- **Path:** `~/Music/Music/Media.localized/Music/`
- **Files:** 0
- **This was the original ~1TB source that was rsynced to Gathering. Already cleaned up.**

### Stray file

- **Path:** `~/Desktop/Desktop - Jeff's Mac mini/01 Rainy Day Women No 12 & 35.m4a`
- **Single Bob Dylan track. Orphan. Delete or move to canonical.**

---

## 3. Bedroom Mac (192.168.86.242) — Findings

### Canonical source — HEALTHY

- **Path:** `/Volumes/VideosNew/Gathering/Music/Music/`
- **Files:** 143,695 audio files (m4a, mp3, flac, m4p)
- **Artist dirs:** 2,790
- **This is the NFS export source. No issues.**

### Previous Libraries — STALE

- **Path:** `~/Music/Music/Previous Libraries.localized/`
- **6 old `.musiclibrary` snapshots:**
  - 2023-04-20, 2023-09-30, 2023-11-17, 2023-12-12, 2024-10-09, 2025-10-02
- **These are just database snapshots (no media files). Safe to delete unless Jeff wants historical library state.**

### ToDo/Library-iTunes — HARVEST GOLD (462GB, 123,905 files)

- **Path:** `/Volumes/VideosNew/Gathering/Music/ToDo/Library-iTunes/`
- **Size: 462GB — this is a FULL music library backup, not just metadata**
- **Contents:**
  - `iTunes Library.itl` — 42MB, binary format (2019 era)
  - `iTunes Music Library.xml` — **170MB** — full XML export with play counts, ratings, date added, skip counts, playlists
  - `iTunes Media/Music/` — 2,662 artist dirs, **123K+ actual audio files** (this is a complete copy of the library as it existed in 2019)
  - `iTunes Media/Audiobooks/` — J.K. Rowling audiobooks
  - 4 Voice Memos
- **Why this matters for harvest:** The 170MB XML is the richest source of play history and ratings. If you're building the music knowledge graph, this XML has:
  - `Play Count` per track
  - `Play Date` / `Play Date UTC`
  - `Rating` (0-100 scale, maps to 0-5 stars)
  - `Skip Count` / `Skip Date`
  - `Date Added`
  - `Persistent ID` for cross-referencing
  - All playlist definitions
- **Dedup note:** 123K files here vs 143K in canonical — significant overlap expected. The canonical set is a superset (added music since 2019). These 462GB are likely ~95%+ duplicates of canonical, but the XML metadata is unique and valuable.
- **Recommendation:** Parse the XML as a harvest source. Map tracks to canonical files by artist/album/title matching. Import play counts and ratings into the knowledge graph as `jb:playCount`, `jb:rating` properties. The 462GB of audio files can be deleted AFTER metadata is harvested and verified against canonical.

### ToDo/Bedroom-mp3 — MIXED BAG (213GB, 51,080 files)

- **Path:** `/Volumes/VideosNew/Gathering/Music/ToDo/Bedroom-mp3/`
- **Size: 213GB — much larger than expected**
- **Contents:**
  - `iTunes/` — 2010-era iTunes library (9MB `.itl`, 63MB `.xml`, 456MB Genius DB). Only 3 artist dirs in its media folder.
  - `iTunes Music/` — 1,401 dirs (likely the actual music folder for the 2010 library — older collection, possibly pre-lossless era, may contain tracks NOT in the canonical set)
  - `Kirby/` — **2,472 entries**. Appears to be someone else's music collection from ~2010. Jeff should confirm whether to keep, merge, or delete.
  - `Outlook/` — **4GB of Outlook PST archives** from 2009-2010. Not music. Stranded personal data — Jeff may want to preserve these elsewhere or delete.
- **Recommendation:** Ask Jeff about Kirby and Outlook. The 2010 iTunes XML (63MB) could be a secondary harvest source for older play history if it covers a different time period than the 2019 XML. The 213GB of audio may contain unique tracks from the pre-lossless era — worth a dedup check against canonical before deleting.

---

## 4. Summary Table

| Location | Machine | Files | Size | Status |
|----------|---------|-------|------|--------|
| `/Volumes/Gathering/Music/Music/` | NFS (Bedroom) | 252,232 | 809GB | Canonical — healthy (includes 34K DRM m4p files) |
| `~/Music/jeff-music/Media.localized/Music/` | Library | 1,753 | 9.2GB | Duplicate — delete after verification |
| `~/Music/Music/Media.localized/Music/` | Library | 0 | 0B | Clean |
| `ToDo/Library-iTunes/` | Bedroom | 123,905 | **462GB** | Full 2019 library backup — harvest XML, then dedup audio |
| `ToDo/Bedroom-mp3/` | Bedroom | 51,080 | **213GB** | 2010-era library + Kirby + Outlook — needs Jeff decision |
| `Previous Libraries.localized/` | Bedroom | metadata | 334MB | Stale — safe to delete |
| Desktop stray | Library | 1 | ~5MB | Orphan — delete |

---

## 5. Questions for Jeff (Before You Act)

1. **jeff-music library:** Is this the active Apple Music library? If so, are tracks referencing NFS or local paths?
2. **Kirby collection:** Keep, merge into canonical, or delete?
3. **Outlook PSTs:** Preserve elsewhere or delete?
4. **Previous Libraries:** OK to delete all 6 snapshots on Bedroom?

---

## 6. Harvest Inputs (for today's work)

If you're building music harvest pipelines today, these are your data sources ranked by value:

1. **Canonical files** — `/Volumes/Gathering/Music/Music/` (143K tracks, already accessible via NFS)
2. **2019 iTunes XML** — `/Volumes/Gathering/Music/ToDo/Library-iTunes/iTunes Music Library.xml` (170MB, richest metadata: play counts, ratings, playlists)
3. **2010 iTunes XML** — `/Volumes/Gathering/Music/ToDo/Bedroom-mp3/iTunes/iTunes Music Library.xml` (63MB, older era, possibly different tracks)
4. **Apple Music library DB** — `~/Music/jeff-music/Music Library.musiclibrary/Library.musicdb` (SQLite, current Apple Music state if jeff-music is active)
