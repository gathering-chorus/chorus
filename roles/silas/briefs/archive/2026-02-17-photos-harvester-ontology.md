# Brief: Photos Harvester — Ontology Design

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-17
**Priority**: P1 — fast-track to Kade
**Card**: (newly created)

---

## Context

Jeff decided to broaden domain coverage rather than go deeper on Music. Apple Photos library is local on the primary Mac — same local-first harvest pattern Kade proved with iTunes. Jeff wants this to move fast: Silas designs the ontology, Kade builds the harvester, Wren and Silas continue Building product work in parallel.

**What we learned from Music that applies here:**
- Local source (JXA/AppleScript for Apple Photos, same as iTunes)
- Metadata + thumbnails only — full-res images stay on disk (C1 disk budget constraint)
- Named graphs per resource type (proven pattern, ADR-008)
- Cross-graph SPARQL joins work at scale (5,800+ albums proved it)
- Deduplication needed (Music had 12k dupes out of 66k)

---

## What We Need From You

### 1. Photos Ontology (classes + properties)

Minimum viable classes:
- **Photo** — the core entity
- **Album** — Apple Photos albums/folders
- **Person** — face recognition tags (Apple Photos assigns these)
- **Location** — GPS coordinates / place names

Key properties to capture:
- Title / filename
- Date taken (EXIF)
- Date modified
- Camera / device
- GPS coordinates → Location
- Dimensions (width × height)
- Album membership
- Person tags (faces)
- Favorite flag
- Media type (photo vs video vs live photo vs screenshot)

### 2. Graph Structure

How should we partition named graphs? Music used separate graphs for albums, artists, tracks. Photos has different relationships — what's the right split?

### 3. Thumbnail Strategy

Photos are visual — unlike music, users need to see them. What's the approach for thumbnails?
- Extract from Apple Photos library at reduced resolution (200×200 like music artwork?)
- Store as base64 in RDF? Or as files with URI references?
- Disk budget: 50k photos × 30KB thumbnail = ~1.5GB. Acceptable?

### 4. Deduplication

Apple Photos may have duplicates (imported from multiple devices, iCloud sync). What's the dedup key? Content hash? Filename + date? EXIF signature?

### 5. What NOT to ingest

We explicitly skip:
- Full-resolution images (stay on disk)
- RAW files
- iCloud-only photos (not downloaded locally)
- Deleted/trash items

---

## Connections to Existing Domains

Photos is richer than Music in terms of cross-domain links:
- **Garden** — photos of plants, beds, seasons (connects to Cultivating)
- **Property** — photos of house, rooms, projects (connects to Harvesting)
- **Family/People** — face tags connect to relationships
- **Travel/Places** — GPS data creates a spatial dimension we don't have yet
- **Time** — date taken creates a temporal dimension (timeline view potential)

These connections are what make Photos strategically important — it's the most connected domain in Jeff's life.

---

## Timeline

Jeff wants this fast. Suggested flow:
1. **Silas**: Ontology design + graph structure (this brief) — 1 session
2. **Kade**: Build harvester reusing Music pattern — 1-2 sessions
3. **Kade**: Browse views (grid, detail, album) — 1 session

Music took ~4 sessions total. Photos should be faster — the pattern is proven.

---

## Not In Scope

- Photo editing / manipulation
- iCloud sync
- Sharing / export
- Face recognition (we consume Apple's tags, we don't run our own)
- Full-text search on photo content (AI vision — future)

---

— Wren
