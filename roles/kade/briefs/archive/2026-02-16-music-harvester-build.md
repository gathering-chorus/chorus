# Brief: Music Collection — Harvester Build

**From**: Wren (PM)
**To**: Kade (Engineer)
**CC**: Silas (Architect) — owes ontology v0.7.0 spec, deliver to Kade directly
**Date**: 2026-02-16
**Priority**: P1 — Now
**Board**: #47

---

## Context

Jeff wants to pressure-test the system by filling one collection with real data at scale. Music won because:
1. Metadata comes free (ID3 tags — artist, album, track, genre, year, duration, play count)
2. Near-zero Jeff manual effort — point at a library, let it run
3. First real harvester — the pattern you build here applies to Photos/Movies/everything after
4. Real scale: 500-5000 tracks expected

## Jeff's Decisions (answered directly)

1. **Normalize artist names** — yes. "The Beatles" and "Beatles" should resolve to one artist entity.
2. **Compilations**: Retain the compilation album name. Show per-track artist AND song title. A compilation album has its own identity; tracks within it have their own artists.
3. **Play count**: Grab it from iTunes. **Skip ratings** — Jeff doesn't use them there.

## What to Build

### 1. Harvester (the reusable pattern)

Two sources:
- **iTunes Library XML** (`iTunes Music Library.xml` or `Library.xml`) — Apple's export format. Contains albums, tracks, artists, playlists, play counts, file paths.
- **Local folder scan** — Read ID3 tags from MP3/M4A/FLAC files. Use `music-metadata` npm package (or similar).

Harvester pipeline:
```
Source → Parse → Normalize → Deduplicate → Write RDF → Sync to Fuseki
```

Key behaviors:
- **Normalize**: Artist name normalization (case, "The" prefix, whitespace). Simple rules, not ML.
- **Deduplicate**: Same track appearing in both iTunes XML and local scan should produce one entity. Match on: artist + album + track title + duration (within tolerance).
- **Idempotent**: Re-running the harvester on the same source updates existing entities, doesn't create duplicates.
- **Logging**: Report counts — tracks found, albums created, artists created, duplicates skipped, errors.

### 2. Data Model (start with this, Silas refines)

Until Silas delivers the full ontology spec, work with this baseline:

- **Artist**: name (normalized), albums (inverse)
- **Album**: title, artist(s), year, genre, track count, cover art URL (if available from iTunes), playCount (sum of tracks)
- **Track**: title, artist(s), album, trackNumber, duration, genre, filePath, playCount
- **Genre**: model as a class (not a string) — enables browse-by-genre
- **MusicCollection**: container for all music entities

When Silas delivers the RDF classes/properties/namespaces, update accordingly. The harvester writes Turtle files to SOLID pods following the existing pattern (one `.ttl` per entity in the appropriate pod directory).

### 3. Browse View

- **Primary**: Album grid — card layout with album title, artist, year, track count. Click to drill down to track list.
- **Secondary**: Track table with sort/filter (artist, album, genre, year). Searchable.
- **Navigation**: Music node in the mind map should link here. Menu slot under Harvesting already exists (currently disabled).

### 4. Admin / Trigger

- **v1**: CLI command or admin page button. Point at iTunes XML path and/or music folder path. Run harvester. Show progress and results.
- **NOT v1**: Scheduled re-harvesting, file watchers, streaming ingestion.

## Scale Considerations

- 500-5000 tracks = 500-5000 Turtle files in pods. Test that this works.
- Fuseki sync is likely needed for browse/filter/search to be performant. Don't rely on pod filesystem reads at scale.
- Pagination for the track table if > 100 results.

## Scope Boundary

**v1 (this card)**: Harvester + data model + browse view + real data loaded. Music is in the system, searchable, browsable.

**NOT v1**: Playback, streaming, playlist management, scrobbling, recommendations, cover art download.

## Silas's Role

Silas owes the full ontology spec (RDF classes, properties, namespaces) — he'll deliver it to you directly. Don't block on it for harvester infrastructure. Start with the baseline model above and swap in his schema when it arrives.

His architectural brief (questions and context): `architect/briefs/2026-02-16-music-harvester-ontology.md`

## What Done Looks Like

- Jeff's music library is ingested (iTunes XML and/or local folder)
- Albums, tracks, artists, genres are browsable in the app
- Music menu item is enabled and linked
- Music node in mind map is active (no longer grayed out)
- Harvester can be re-run without creating duplicates

---

— Wren
