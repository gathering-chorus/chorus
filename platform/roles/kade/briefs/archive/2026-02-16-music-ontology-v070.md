# Music Ontology v0.7.0 + Harvester Architecture

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-16
**Priority**: P1 — you're building now, this is the spec
**Board**: #47, #12 (folded in)

---

## Overview

First real harvester. 66k+ tracks on this machine alone, multi-source from day one. Sets the pattern for Photos/Movies/everything after. Data provenance is a first-class requirement.

**Source**: Apple Music app via JXA (JavaScript for Automation). No file scanning, no ID3 parsing. Query the app directly.

**Jeff's decisions**: Normalize artists, retain compilation album names with per-track artist, grab play count, skip ratings.

---

## 1. Ontology Design

Add to `src/ontology/jb-ontology.ttl`. Bump version to `0.7.0`.

### Classes

```turtle
# --- Music Domain ---

jb:MusicCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Music Collection" ;
    rdfs:comment "Collection of music albums, tracks, and artists." .

jb:Album a owl:Class ;
    rdfs:label "Album" ;
    rdfs:comment "A music album — a named collection of tracks by one or more artists." .

jb:Track a owl:Class ;
    rdfs:label "Track" ;
    rdfs:comment "A single music track within an album." .

jb:Artist a owl:Class ;
    rdfs:label "Artist" ;
    rdfs:comment "A music artist or band. Canonical — normalized name, one entity per artist." .

jb:Genre a owl:Class ;
    rdfs:label "Genre" ;
    rdfs:comment "A music genre. Modeled as class (not string) for browse-by-genre queries." .

# --- Harvester Infrastructure (generic — reuse for Photos/Movies/etc.) ---

jb:HarvestSource a owl:Class ;
    rdfs:label "Harvest Source" ;
    rdfs:comment "A machine + application that provides harvestable data. Multi-source: same content may exist in multiple sources." .

jb:HarvestRun a owl:Class ;
    rdfs:label "Harvest Run" ;
    rdfs:comment "A single execution of a harvester against a source. Every harvested resource links to its harvest run for provenance." .
```

### Object Properties

```turtle
# --- Music relationships ---

jb:hasAlbum a owl:ObjectProperty ;
    rdfs:label "has album" ;
    rdfs:domain jb:MusicCollection ;
    rdfs:range jb:Album ;
    rdfs:comment "Collection contains this album." .

jb:hasTrack a owl:ObjectProperty ;
    rdfs:label "has track" ;
    rdfs:domain jb:Album ;
    rdfs:range jb:Track ;
    rdfs:comment "Album contains this track." .

jb:inAlbum a owl:ObjectProperty ;
    rdfs:label "in album" ;
    rdfs:domain jb:Track ;
    rdfs:range jb:Album ;
    owl:inverseOf jb:hasTrack ;
    rdfs:comment "Track belongs to this album." .

jb:byArtist a owl:ObjectProperty ;
    rdfs:label "by artist" ;
    rdfs:comment "The artist of this album or track. Track-level artist may differ from album artist (compilations)." ;
    rdfs:range jb:Artist .
    # domain: Album or Track (not restricted to allow reuse)

jb:albumArtist a owl:ObjectProperty ;
    rdfs:label "album artist" ;
    rdfs:domain jb:Album ;
    rdfs:range jb:Artist ;
    rdfs:comment "The primary artist of the album. For compilations, this is 'Various Artists' (a real Artist entity)." .

jb:hasGenre a owl:ObjectProperty ;
    rdfs:label "has genre" ;
    rdfs:range jb:Genre ;
    rdfs:comment "Genre classification. Applies to albums or tracks." .

# --- Provenance relationships (generic) ---

jb:harvestedIn a owl:ObjectProperty ;
    rdfs:label "harvested in" ;
    rdfs:comment "The harvest run that created or last updated this resource." ;
    rdfs:range jb:HarvestRun .

jb:fromSource a owl:ObjectProperty ;
    rdfs:label "from source" ;
    rdfs:domain jb:HarvestRun ;
    rdfs:range jb:HarvestSource ;
    rdfs:comment "The source this harvest run read from." .

jb:hasProvenance a owl:ObjectProperty ;
    rdfs:label "has provenance" ;
    rdfs:comment "Links a resource to all sources it has been harvested from (multi-source support)." ;
    rdfs:range jb:HarvestSource .
```

### Datatype Properties

```turtle
# --- Music metadata ---

jb:trackNumber a owl:DatatypeProperty ;
    rdfs:label "track number" ;
    rdfs:domain jb:Track ;
    rdfs:range xsd:integer .

jb:discNumber a owl:DatatypeProperty ;
    rdfs:label "disc number" ;
    rdfs:domain jb:Track ;
    rdfs:range xsd:integer .

jb:duration a owl:DatatypeProperty ;
    rdfs:label "duration" ;
    rdfs:comment "Duration in seconds." ;
    rdfs:range xsd:decimal .

jb:year a owl:DatatypeProperty ;
    rdfs:label "year" ;
    rdfs:comment "Release year." ;
    rdfs:range xsd:integer .

jb:playCount a owl:DatatypeProperty ;
    rdfs:label "play count" ;
    rdfs:comment "Number of times played. Harvested from Apple Music. Source-specific — may differ per machine." ;
    rdfs:range xsd:integer .

jb:isCompilation a owl:DatatypeProperty ;
    rdfs:label "is compilation" ;
    rdfs:domain jb:Album ;
    rdfs:range xsd:boolean ;
    rdfs:comment "True if album is a compilation (various artists)." .

jb:coverArt a owl:DatatypeProperty ;
    rdfs:label "cover art" ;
    rdfs:comment "Relative path to album cover image file." ;
    rdfs:range xsd:string .

jb:normalizedName a owl:DatatypeProperty ;
    rdfs:label "normalized name" ;
    rdfs:comment "Lowercased, trimmed, article-stripped version of a name. Used as dedup key." ;
    rdfs:range xsd:string .

# --- Provenance metadata (generic) ---

jb:sourceMachine a owl:DatatypeProperty ;
    rdfs:label "source machine" ;
    rdfs:domain jb:HarvestSource ;
    rdfs:range xsd:string ;
    rdfs:comment "Machine name (e.g., 'Jeff\\'s Mac Mini M1')." .

jb:sourceType a owl:DatatypeProperty ;
    rdfs:label "source type" ;
    rdfs:domain jb:HarvestSource ;
    rdfs:range xsd:string ;
    rdfs:comment "Type of source application (e.g., 'apple-music', 'local-files', 'spotify')." .

jb:harvesterVersion a owl:DatatypeProperty ;
    rdfs:label "harvester version" ;
    rdfs:domain jb:HarvestRun ;
    rdfs:range xsd:string .

jb:harvestStarted a owl:DatatypeProperty ;
    rdfs:label "harvest started" ;
    rdfs:domain jb:HarvestRun ;
    rdfs:range xsd:dateTime .

jb:harvestCompleted a owl:DatatypeProperty ;
    rdfs:label "harvest completed" ;
    rdfs:domain jb:HarvestRun ;
    rdfs:range xsd:dateTime .

jb:harvestItemCount a owl:DatatypeProperty ;
    rdfs:label "harvest item count" ;
    rdfs:domain jb:HarvestRun ;
    rdfs:range xsd:integer .

jb:sourceRawValue a owl:DatatypeProperty ;
    rdfs:label "source raw value" ;
    rdfs:comment "The original value from the source before normalization. Provenance: what did the source actually say?" ;
    rdfs:range xsd:string .
```

---

## 2. URI Patterns

Stable URIs are critical for multi-source dedup.

```
# Sources
/music/sources/{machine-slug}
  e.g., /music/sources/mac-mini-m1

# Harvest runs
/music/harvests/{timestamp}
  e.g., /music/harvests/2026-02-16T11-00-00

# Artists (canonical — normalized name is the key)
/music/artists/{normalized-name-slug}
  e.g., /music/artists/beatles
  e.g., /music/artists/various-artists

# Albums (artist-slug + album-slug)
/music/albums/{artist-slug}/{album-slug}
  e.g., /music/albums/beatles/abbey-road
  e.g., /music/albums/various-artists/now-thats-what-i-call-music-47

# Tracks (album-uri + disc-track)
/music/tracks/{artist-slug}/{album-slug}/{disc}-{track}
  e.g., /music/tracks/beatles/abbey-road/1-01

# Genres
/music/genres/{genre-slug}
  e.g., /music/genres/rock
  e.g., /music/genres/electronic
```

### Slug generation rules

```typescript
function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/^(the|a|an)\s+/i, '')  // strip leading articles
    .replace(/[^a-z0-9]+/g, '-')     // non-alphanum → hyphen
    .replace(/^-|-$/g, '');           // trim leading/trailing hyphens
}
```

### Dedup key (cross-source identity)

A track is the same track across sources if:
```
normalize(artist) + normalize(album) + normalize(title) + round(duration, 1)
```

This composite key handles: different file paths, different Apple persistent IDs, slight metadata variations. The `round(duration, 1)` allows for tiny encoding differences.

---

## 3. Harvester Architecture

### Source: Apple Music via JXA

**NOT file scanning. NOT ID3 parsing.** Query Apple Music directly.

```javascript
// harvest-music.js — run via: osascript -l JavaScript harvest-music.js
const Music = Application('Music');
const tracks = Music.tracks();

for (const track of tracks) {
  const data = {
    name: track.name(),
    artist: track.artist(),
    albumArtist: track.albumArtist(),
    album: track.album(),
    genre: track.genre(),
    year: track.year(),
    trackNumber: track.trackNumber(),
    discNumber: track.discNumber(),
    duration: track.duration(),
    playCount: track.playedCount(),
    compilation: track.compilation(),
    // Artwork: track.artworks[0] (extract separately — binary)
  };
  // ... write to stdout as JSON lines
}
```

**Why JXA over AppleScript**: JavaScript, fits the Node stack, handles structured output natively, no string escaping hell.

**Architecture**:

```
┌────────────────┐     ┌──────────────────┐     ┌──────────────┐
│  Apple Music   │────→│  JXA Harvester   │────→│  JSON Lines   │
│  (66k tracks)  │     │  (osascript -l   │     │  (stdout)     │
│                │     │   JavaScript)    │     │               │
└────────────────┘     └──────────────────┘     └──────┬────────┘
                                                       │
                                                       ▼
                                                ┌──────────────┐
                                                │  Ingester    │
                                                │  (Node.js)   │
                                                │              │
                                                │  - Normalize │
                                                │  - Dedup     │
                                                │  - Write TTL │
                                                │  - Extract   │
                                                │    artwork   │
                                                │  - Sync to   │
                                                │    Fuseki    │
                                                └──────────────┘
```

**Two-phase design**:
1. **Extract** (JXA): Pull raw data from Apple Music → JSON lines to stdout. Pure extraction, no transformation.
2. **Ingest** (Node.js): Read JSON lines, normalize, deduplicate, write Turtle to pods, extract artwork, sync to Fuseki.

**Why two phases**: The JXA script runs in Apple's scripting environment. The ingester runs in Node where we have full access to the pod services, Fuseki client, and slug generation. Clean separation. The extract phase is source-specific (Apple Music today, Spotify adapter tomorrow). The ingest phase is generic.

### Multi-source handling

```
Harvest from Mac Mini M1:
  → Extract 66k tracks as JSON
  → Ingest: create/update Artist, Album, Track resources
  → Each resource gets: jb:harvestedIn <run-1>, jb:hasProvenance <mac-mini-m1>

Harvest from MacBook (later):
  → Extract N tracks as JSON
  → Ingest: for each track, compute dedup key
    → If exists: merge metadata (take richest), add jb:hasProvenance <macbook>
    → If new: create resource, link provenance
  → Play counts: keep per-source (sum? max? Jeff decides later)
```

### Artwork extraction

```
For each unique album:
  → Check if artwork exists in Apple Music (track.artworks.length > 0)
  → Extract JPEG via JXA
  → Save to public/images/albums/{album-slug}.jpg (full size)
  → Generate thumbnail: public/images/albums/{album-slug}-thumb.jpg (200x200)
  → Set jb:coverArt on Album resource
```

Use `sharp` (npm) for thumbnail generation — already a common Node image library, no heavy dependencies.

### Admin page (not CLI)

Build as an admin page in the app: `/admin/harvest/music`

- **Start harvest** button (triggers extract → ingest pipeline)
- **Progress bar** (tracks processed / total)
- **Source selector** (which machine/library to harvest from — for now just "this machine")
- **Results**: albums created, tracks created, artists created, duplicates merged, errors
- **History**: list of previous harvest runs with timestamps and counts

This is better than a CLI script because:
- Jeff can trigger it from the app (no terminal needed)
- Progress is visible
- History is browsable
- Future: schedule periodic re-harvests

### Batch strategy

66k tracks is too many for one-shot. Batch in chunks of 500:

```
Extract all tracks → JSON lines file (temporary)
Read file in 500-line batches:
  → Normalize + dedup batch
  → Write Turtle files
  → Sync to Fuseki
  → Update progress
  → Next batch
```

Estimated time: ~5-10 minutes for full harvest (depends on Fuseki sync speed). First run is slowest (all creates). Re-harvests are faster (mostly dedup matches).

---

## 4. Storage Pattern

**NOT one Turtle file per track.** At 66k tracks that would kill the filesystem.

**Pattern B: Aggregate Turtle — one file per album, tracks embedded.**

```
/pods/{webId}/music/
├── .meta.ttl                     (collection metadata, visibility: Private)
├── sources/
│   └── mac-mini-m1.ttl           (HarvestSource instance)
├── harvests/
│   └── 2026-02-16T11-00-00.ttl   (HarvestRun instance)
├── artists/
│   ├── beatles.ttl                (Artist instance + albums list)
│   ├── various-artists.ttl
│   └── ...
├── albums/
│   ├── beatles/
│   │   └── abbey-road.ttl         (Album instance + all Track instances)
│   ├── various-artists/
│   │   └── now-thats-what-i-call-music-47.ttl
│   └── ...
├── genres/
│   ├── rock.ttl
│   ├── electronic.ttl
│   └── ...
└── artwork/
    ├── abbey-road.jpg
    ├── abbey-road-thumb.jpg
    └── ...
```

**Why per-album, not per-track**: An album with 12 tracks is one file with ~50-80 triples. That's tiny. 4,000 album files is manageable. 66,000 track files is not.

**Why not one giant file**: Albums can be individually updated (re-harvest adds play counts), individually visible (graduation model), and individually cached.

### Example album Turtle

```turtle
@prefix jb: <https://jeffbridwell.com/ontology#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

</music/albums/beatles/abbey-road> a jb:Album ;
    dcterms:title "Abbey Road" ;
    jb:albumArtist </music/artists/beatles> ;
    jb:year 1969 ;
    jb:hasGenre </music/genres/rock> ;
    jb:isCompilation false ;
    jb:coverArt "artwork/abbey-road.jpg" ;
    jb:harvestedIn </music/harvests/2026-02-16T11-00-00> ;
    jb:hasProvenance </music/sources/mac-mini-m1> ;
    jb:hasTrack </music/tracks/beatles/abbey-road/1-01>,
                </music/tracks/beatles/abbey-road/1-02> .

</music/tracks/beatles/abbey-road/1-01> a jb:Track ;
    dcterms:title "Come Together" ;
    jb:byArtist </music/artists/beatles> ;
    jb:inAlbum </music/albums/beatles/abbey-road> ;
    jb:trackNumber 1 ;
    jb:discNumber 1 ;
    jb:duration 259.0 ;
    jb:playCount 47 ;
    jb:hasGenre </music/genres/rock> ;
    jb:harvestedIn </music/harvests/2026-02-16T11-00-00> .

</music/tracks/beatles/abbey-road/1-02> a jb:Track ;
    dcterms:title "Something" ;
    jb:byArtist </music/artists/beatles> ;
    jb:inAlbum </music/albums/beatles/abbey-road> ;
    jb:trackNumber 2 ;
    jb:discNumber 1 ;
    jb:duration 183.0 ;
    jb:playCount 32 ;
    jb:hasGenre </music/genres/rock> ;
    jb:harvestedIn </music/harvests/2026-02-16T11-00-00> .
```

---

## 5. Artist Normalization Rules

```typescript
function normalizeArtistName(raw: string): string {
  let name = raw.trim();
  // Strip leading articles
  name = name.replace(/^(The|A|An)\s+/i, '');
  // Collapse whitespace
  name = name.replace(/\s+/g, ' ');
  // Handle "Last, First" → "First Last" (optional — only if pattern detected)
  // Keep as-is for band names
  return name;
}

function artistSlug(raw: string): string {
  return slugify(normalizeArtistName(raw));
}
```

**Store both**: `dcterms:title "The Beatles"` (display name, original) + `jb:normalizedName "beatles"` (dedup key). The raw source value goes in `jb:sourceRawValue` on the provenance link.

**"Various Artists"** is a real Artist entity: `/music/artists/various-artists`. Albums with `jb:isCompilation true` link to this artist via `jb:albumArtist`. Individual tracks link to their actual artist via `jb:byArtist`.

---

## 6. Fuseki Sync

**Required at this scale.** Filesystem scanning can't support search/filter across 66k tracks.

Sync pattern:
- After each batch of albums is written to pods, bulk-load the Turtle into Fuseki
- Use Fuseki's Graph Store Protocol: `PUT /dataset/data?graph=<music-graph>` for bulk updates
- Keep a named graph for music: `<https://jeffbridwell.com/graphs/music>`
- Re-harvest: clear and reload the graph (simpler than incremental sync at this scale)

### Key SPARQL queries the browse view needs

```sparql
# All albums, sorted by artist
SELECT ?album ?title ?artist ?artistName ?year ?coverArt
WHERE {
  ?album a jb:Album ;
    dcterms:title ?title ;
    jb:albumArtist ?artist ;
    jb:year ?year .
  ?artist dcterms:title ?artistName .
  OPTIONAL { ?album jb:coverArt ?coverArt }
}
ORDER BY ?artistName ?year

# Tracks for an album
SELECT ?track ?title ?artist ?artistName ?trackNum ?duration ?playCount
WHERE {
  ?track a jb:Track ;
    jb:inAlbum </music/albums/beatles/abbey-road> ;
    dcterms:title ?title ;
    jb:byArtist ?artist ;
    jb:trackNumber ?trackNum ;
    jb:duration ?duration .
  ?artist dcterms:title ?artistName .
  OPTIONAL { ?track jb:playCount ?playCount }
}
ORDER BY ?trackNum

# All tracks by an artist (across albums, including compilations)
SELECT ?track ?title ?album ?albumTitle ?trackNum
WHERE {
  ?track a jb:Track ;
    jb:byArtist </music/artists/beatles> ;
    dcterms:title ?title ;
    jb:inAlbum ?album ;
    jb:trackNumber ?trackNum .
  ?album dcterms:title ?albumTitle .
}
ORDER BY ?albumTitle ?trackNum

# Browse by genre
SELECT ?album ?title ?artist ?artistName ?coverArt
WHERE {
  ?album a jb:Album ;
    jb:hasGenre </music/genres/rock> ;
    dcterms:title ?title ;
    jb:albumArtist ?artist .
  ?artist dcterms:title ?artistName .
  OPTIONAL { ?album jb:coverArt ?coverArt }
}
ORDER BY ?artistName

# Most played tracks
SELECT ?track ?title ?artist ?artistName ?playCount
WHERE {
  ?track a jb:Track ;
    dcterms:title ?title ;
    jb:byArtist ?artist ;
    jb:playCount ?playCount .
  ?artist dcterms:title ?artistName .
}
ORDER BY DESC(?playCount)
LIMIT 50
```

---

## 7. Browse View

**Primary**: Album grid (cover art cards) — scannable, visual. Click → album detail with track list.
**Secondary**: Artist list with album grouping. Track table with sort/filter (by artist, genre, play count).

### Album grid page (`/music` or `/music/albums`)
- Cover art thumbnails in a responsive grid
- Album title + artist below each card
- Filter: by genre (dropdown), by artist (search/autocomplete)
- Sort: by artist name (default), by year, by recently added
- Pagination: 50 albums per page (with cover art, keep it performant)

### Album detail page (`/music/albums/:artist/:album`)
- Full-size cover art
- Album metadata (title, artist, year, genre, track count, total duration)
- Track table: #, title, artist (if differs from album), duration, play count
- Provenance: "Harvested from Mac Mini M1 on Feb 16, 2026"

### Artist page (`/music/artists/:artist`)
- All albums by this artist (grid)
- All tracks by this artist across albums (table)
- Total play count across all tracks

---

## 8. What NOT to Build

- No playback / streaming
- No playlist management
- No scrobbling / last.fm integration
- No MusicBrainz enrichment (future)
- No ratings (Jeff doesn't use them)
- No lyrics
- No Spotify integration (future harvester adapter)

---

## 9. Scale Notes

- **66k tracks ÷ ~15 tracks/album ≈ 4,400 albums** — manageable as individual Turtle files
- **Fuseki**: 66k tracks × ~8 triples/track + 4.4k albums × ~10 triples/album ≈ 572k triples. Well within Fuseki's capacity (ceiling: 15-25M triples). No optimization needed yet.
- **Artwork**: 4,400 albums × ~50KB/thumbnail ≈ 220MB. Store in `public/images/albums/`. Serve statically.
- **Harvest time**: ~5-10 minutes for full initial harvest. JXA query is the bottleneck (~1-2 sec per 1000 tracks).

---

## 10. This Is the Harvester Pattern

The architecture above is **generic**. For future harvesters:

| Domain | Extract Phase | Ingest Phase |
|---|---|---|
| **Music** | JXA → Apple Music → JSON lines | Normalize, dedup, write Turtle, sync Fuseki |
| **Photos** | EXIF → local files → JSON lines | Same ingester pattern, different ontology classes |
| **Movies** | File metadata → JSON lines | Same pattern |
| **Books** | Already in pods (manual) | Already done |
| **Spotify** | API → JSON lines | Same ingester, different extract adapter |

The **extract** phase is source-specific. The **ingest** phase is generic. Build the ingester as a reusable service.

— Silas
