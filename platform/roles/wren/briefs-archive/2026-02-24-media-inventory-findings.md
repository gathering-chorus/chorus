# Media Rescue — Technical Inventory Findings

**From**: Kade (Engineer) → Wren (PM)
**Re**: #311 — Media rescue inventory
**Date**: 2026-02-24

## 1. Media Landscape Inventory

### Primary Mac (local, 192.168.86.36)

| Location | Size | m4a | mp3 | Other | Notes |
|----------|------|-----|-----|-------|-------|
| `~/Downloads/iTunes` | 462 GB | 79,769 | 19,224 | 3 m4v | Full iTunes library export with XML, artwork, mobile apps |
| `~/Downloads/Music` | 481 GB | 79,574 | 18,480 | — | Apple Music library export (Music/ subfolder) |
| `~/Music/Music` | 484 GB | 79,891 | 18,481 | — | **Active** Apple Music library (Media.localized + .musiclibrary) |
| `~/Downloads/Pictures` | 44 GB | — | — | 16,621 jpg, 236 heic, 210 png | ~18,796 total files |
| `~/Pictures/Photos Library` | present | — | — | — | ~44,193 files (Apple Photos SQLite source) |

**Jeff is currently copying iTunes, Music, Pictures from Downloads to /Gathering on secondary Mac.** ETA ~24h. This data will be the harvest target.

### Secondary Mac (192.168.86.33, SMB at /Volumes/Gathering)

| Location | Size | Files | Notes |
|----------|------|-------|-------|
| `/Volumes/Gathering/iTunes` | 25 GB | 4,606 m4a | Partial — has iTunes Media/Music + XML (170MB library XML) |
| `/Volumes/Gathering/Music` | 16 KB | — | Empty placeholder |
| `/Volumes/Gathering/music` | 72 KB | — | Empty placeholder |
| `/Volumes/Gathering/Pictures` | 16 KB | — | Empty placeholder |
| `/Volumes/Gathering/video` | 549 GB | — | Video content (kenna-james, kenzie-reeves dirs visible) |
| `/Volumes/Gathering/backups` | 16 KB | — | Empty placeholder |

**After copy completes**, /Gathering will hold the full iTunes + Music + Pictures from primary Mac's Downloads. This is the consolidation target.

## 2. Duplication Estimate

Compared unique m4a basenames across the 3 primary Mac music locations:

| Pair | Overlap | % of smaller set |
|------|---------|-----------------|
| iTunes ∩ Music | 74,491 | 97.6% |
| iTunes ∩ Apple Music | 74,331 | 96.9% |
| Music ∩ Apple Music | 75,726 | 98.8% |
| **All three** | **73,910** | **~96%** |

**Conclusion: These are essentially the same library copied 3 times.** ~74K tracks appear in all three locations. The remaining ~3K unique basenames per location likely represent version drift (purchased/added at different times, re-encodes, or filename normalization differences).

Total unique m4a basenames across all 3: ~76,800. This is **one music collection** stored three times, consuming ~1.4 TB.

The 25 GB on the SMB mount (4,606 m4a) looks like an older/partial copy — likely pre-migration.

## 3. Harvester Coverage Map

### Music Harvester (v1.0.0)
**Source**: Apple Music app via JXA (`osascript`, `harvest-apple-music.js`)

**Fields indexed**: name, artist, albumArtist, album, genre, year, trackNumber, discNumber, duration, playCount, compilation, artworkBase64

**Dedup key**: `trackDedupKey(artist, album, name, duration)` — metadata-based, not file-based.

**Missing**:
- **Source file path** — JXA's `t.location()` property is available in the Apple Music API but **never called** in the extraction script. This is the key gap.
- **File hash / fingerprint** — No content-based dedup identifier. Dedup is purely metadata (artist+album+name+duration), which can't distinguish re-encodes or different bitrate versions of the same track.
- **Bitrate / codec / file size** — Not extracted. Can't tell AAC 256 from AAC 128 or lossless.
- **Date added / date modified** — Not extracted. Can't determine which copy is the "original."
- **Apple Music persistent ID** — Available in JXA, not captured.

### Photos Harvester (v2.1.0)
**Source**: Apple Photos SQLite database (direct read, no JXA)

**Fields indexed**: uuid, filename, dateTaken, mediaType, mediaSubtype, width, height, favorite, latitude, longitude, description, keywords, albums, personSlugs, thumbnailPath

**Dedup key**: `photoDedupKey(filename, date)` or `photoFallbackDedupKey(date, width, height, mediaType)` for generic filenames (IMG_*, DSC*).

**Missing**:
- **Source file path** — The SQLite query **extracts `ZDIRECTORY`** from the ZASSET table but **drops it** in the mapping to `PhotoItemRaw`. The interface has no `directory` or `sourcePath` field. This is a 2-line fix.
- **File hash** — Not extracted. Apple Photos stores `ZADJUSTEDFINGERPRINT` and `ZORIGINALFINGERPRINT` in SQLite — these are content hashes. Available but untapped.
- **Original file size** — `ZORIGINALFILESIZE` is in the SQLite schema. Not extracted.
- **Import session** — `ZIMPORTSESSION` links photos to the import event. Could identify which batch a photo came from.

### Notes Harvester (present), WordPress Harvester (present)
Not media — not in scope for this rescue. Included for completeness.

### Books Pod (present)
Only a pod service, no harvester. Manual upload via UI.

## 4. Gap Analysis — Source Path Tracking

### Music: Medium effort
- **JXA change**: Add `location: t.location()` to the extraction script (~1 line). The `location()` property returns the POSIX file path of the track.
- **Interface change**: Add `location?: string` to `MusicTrackRaw` (~1 line).
- **Ingestion change**: Thread `location` through to `TrackResource` and into the Turtle output (~5 lines across music-pod.service.ts).
- **Ontology**: Add `jb:sourceFilePath` predicate.
- **Risk**: `t.location()` may return `undefined` for Apple Music streaming tracks (no local file). Need to handle gracefully. Also, JXA access to Music app is slow — 98K tracks already takes significant time. Adding `location()` adds one property access per track.
- **Estimate**: ~30 min code change, plus testing and a harvest run (~1-2h depending on library size).

### Photos: Small effort (2-line fix)
- **Interface change**: Add `directory?: string` to `PhotoItemRaw` (~1 line).
- **SQLite mapping**: Include `directory: row.directory` in the `rows.map()` at photo-sqlite.service.ts:135 (~1 line).
- **Ingestion change**: Thread through to `PhotoResource` and Turtle output (~5 lines).
- **The data is already being queried from SQLite — it's just being dropped on the floor.**
- **Bonus**: Can also extract `ZADJUSTEDFINGERPRINT` (content hash) and `ZORIGINALFILESIZE` from the same SQLite query for free.
- **Estimate**: ~15 min code change.

### Both: Ontology alignment needed
Silas should weigh in on the predicate naming. Suggestion: `jb:sourceFilePath` (string, the absolute path) and `jb:sourceFileHash` (string, content fingerprint). This connects to the broader question of how Gathering links back to playable/viewable content — Jeff's stated goal.

## 5. Recommendation

1. **Don't harvest yet.** Wait for the copy to /Gathering to complete. Then we have one authoritative location.
2. **Add source-path tracking first** (both harvesters). Small change, huge value — it's the difference between "I know I have this song" and "I can play this song."
3. **Add content hashing to photos** — free data from SQLite, enables true dedup across libraries.
4. **Music dedup needs a plan.** With 96% overlap across 3 copies, we need a strategy: pick one canonical location, harvest from there, verify the others are subsets. The iTunes XML (170MB on SMB) may contain location data too — worth parsing as a secondary source.
5. **Brief to Silas** for ontology predicates and storage topology guidance (he did the Fuseki audit, knows the disk story).
