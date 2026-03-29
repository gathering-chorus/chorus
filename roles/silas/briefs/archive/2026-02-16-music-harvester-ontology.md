# Brief: Music Collection — Ontology + Harvester Architecture

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-16
**Priority**: Next (Jeff approved, P1)
**Board**: #47 (music harvester), #12 (music ontology — existing card, fold into this)

---

## Context

Jeff wants to pressure-test the system by filling one collection with real data at scale. Music won because:
1. Metadata comes free (ID3 tags — artist, album, track, genre, year, duration)
2. You already have Album/Track/Artist queued as ontology Gap 3
3. This is the first real harvester — sets the pattern for Photos/Movies/etc.
4. Near-zero Jeff manual effort — point at a library, let it run

Jeff has music in **iTunes** and **local files**. Goal: ingest hundreds to thousands of tracks with full metadata into SOLID pods, browse/search/filter in the app.

## What I Need From You

### 1. Ontology Design (v0.7.0)

Design the music domain classes and properties. At minimum:
- **Album**: title, artist(s), year, genre, cover art URL, track count
- **Track**: title, artist(s), album, track number, duration, genre, file path/URL
- **Artist**: name, albums (inverse)
- **MusicCollection**: collection container

Questions:
- Do we model Genre as a class (with instances) or a string property? Class gives us browsing by genre, string is simpler.
- How do we handle compilations / various artists? A track can have a different artist than its album.
- Do we store the actual audio file reference, or just metadata? (I'd say metadata only for v1 — we're testing the data pipeline, not building a music player.)

### 2. Harvester Architecture

Design the harvester pattern that Kade builds. This is the FIRST harvester, so the pattern matters — Photos/Movies will follow it.

Sources to support:
- **iTunes Library XML** (`iTunes Music Library.xml` or `Library.xml`) — Apple's export format. Contains albums, tracks, artists, playlists, play counts, ratings, file paths.
- **Local folder scan** — Read ID3 tags from MP3/M4A/FLAC files via a Node library (e.g., `music-metadata`)

Questions:
- Does the harvester write directly to pods, or go through the CaptureItem pipeline (like SMS)?
- How do we handle duplicates on re-harvest? (Same track from iTunes XML and local scan)
- Should the harvester be a one-time script, a CLI command, or an admin page in the app?
- Does this follow the existing content-ingestion-matrix pattern?

### 3. Browse View Spec

What does the music browse page look like? Candidates:
- Album grid (cover art cards) with drill-down to tracks
- Artist list with album grouping
- Track table with sort/filter

I'd say: album grid as primary view (visual, scannable), track table as secondary. But you know the data model better.

### 4. Scale Considerations

Jeff might have 500-5000 tracks. That's 500-5000 individual Turtle files in the pod. Questions:
- Does the current pod filesystem pattern hold at that scale?
- Do we need Fuseki sync for music to make search/filter performant?
- Any pagination concerns for the browse view?

## Scope Boundary

**v1 (this card)**: Ontology + harvester + browse view + real data loaded. Music plays in the system, fully searchable and browsable.
**NOT v1**: Playback, streaming, playlist management, scrobbling, recommendations.

---

— Wren
