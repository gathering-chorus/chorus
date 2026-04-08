# Photos Ontology — v0.8.0

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-17
**Priority**: P1 — build now
**Card**: Photos harvester (Wren creating)

---

## Ontology Version Bump: v0.7.0 → v0.8.0

Photos is the second harvester domain. Follows the patterns proven in Music (v0.7.0).

---

## Classes

```turtle
@prefix jb: <http://gathering.jeffbridwell.com/ontology#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Core entity
jb:Photo a owl:Class ;
    rdfs:label "Photo" ;
    rdfs:comment "A photo, video, live photo, or screenshot from Apple Photos" .

# Container
jb:PhotoAlbum a owl:Class ;
    rdfs:label "Photo Album" ;
    rdfs:comment "An Apple Photos album or folder" .

# People (face recognition tags)
jb:PhotoPerson a owl:Class ;
    rdfs:label "Photo Person" ;
    rdfs:comment "A person identified via Apple Photos face recognition" .

# Location (GPS-derived)
jb:PhotoLocation a owl:Class ;
    rdfs:label "Photo Location" ;
    rdfs:comment "A place derived from photo GPS coordinates" .

# Collection container
jb:PhotoCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Photo Collection" ;
    rdfs:comment "Top-level container for the photos domain" .
```

---

## Properties

### Photo properties
```turtle
# Identity
jb:photoFilename a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:string ;
    rdfs:comment "Original filename" .

jb:photoMediaType a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:string ;
    rdfs:comment "Type: photo, video, live-photo, screenshot, selfie, panorama" .

# Temporal
jb:dateTaken a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:dateTime ;
    rdfs:comment "EXIF date taken" .

jb:dateModified a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:dateTime ;
    rdfs:comment "Last modification date" .

# Technical
jb:cameraDevice a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:string ;
    rdfs:comment "Camera or device name (e.g. iPhone 14 Pro)" .

jb:imageWidth a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:integer .

jb:imageHeight a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:integer .

# Spatial
jb:latitude a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:decimal .

jb:longitude a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:decimal .

jb:atLocation a owl:ObjectProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range jb:PhotoLocation ;
    rdfs:comment "GPS-derived location" .

# Personal
jb:isFavorite a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:boolean .

# Relationships
jb:inAlbum a owl:ObjectProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range jb:PhotoAlbum ;
    rdfs:comment "A photo can be in multiple albums" .

jb:hasPerson a owl:ObjectProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range jb:PhotoPerson ;
    rdfs:comment "Face tag — a photo can have multiple people" .

# Thumbnail reference
jb:thumbnailPath a owl:DatatypeProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range xsd:string ;
    rdfs:comment "Path to 200x200 JPEG thumbnail" .

# Provenance (reuse from Music v0.7.0)
jb:harvestedBy a owl:ObjectProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range jb:HarvestRun .
```

### Album properties
```turtle
jb:albumTitle a owl:DatatypeProperty ;
    rdfs:domain jb:PhotoAlbum ;
    rdfs:range xsd:string .

jb:hasPhoto a owl:ObjectProperty ;
    rdfs:domain jb:PhotoAlbum ;
    rdfs:range jb:Photo ;
    owl:inverseOf jb:inAlbum .
```

### Person properties
```turtle
jb:personName a owl:DatatypeProperty ;
    rdfs:domain jb:PhotoPerson ;
    rdfs:range xsd:string ;
    rdfs:comment "Name from Apple Photos face recognition" .
```

### Location properties
```turtle
jb:locationName a owl:DatatypeProperty ;
    rdfs:domain jb:PhotoLocation ;
    rdfs:range xsd:string ;
    rdfs:comment "Place name (city, neighborhood, landmark)" .

jb:locationLatitude a owl:DatatypeProperty ;
    rdfs:domain jb:PhotoLocation ;
    rdfs:range xsd:decimal .

jb:locationLongitude a owl:DatatypeProperty ;
    rdfs:domain jb:PhotoLocation ;
    rdfs:range xsd:decimal .
```

---

## URI Patterns

```
# Photos — by date bucket (YYYY/MM)
/photos/items/2024/06/{normalized-filename}

# Albums
/photos/albums/{normalized-album-name}

# People
/photos/people/{normalized-person-name}

# Locations
/photos/locations/{normalized-place-name}
```

**Normalization**: Same rules as Music — lowercase, hyphens for spaces, strip special chars.

**Date bucketing**: Photos are bucketed by year/month from `dateTaken`. This keeps per-directory file counts manageable (vs. one flat directory of 50k files).

---

## Graph Structure

Follow Music's Pattern B (per-container Turtle files) and ADR-008 (cross-graph joins).

| Entity | Graph Pattern | Example |
|---|---|---|
| Photos | Per-album Turtle file | `/photos/albums/garden-2024.ttl` contains all photos in that album |
| Unalbummed photos | Per-month Turtle file | `/photos/unalbummed/2024-06.ttl` |
| Albums | Aggregate album index | `/photos/albums/index.ttl` |
| People | One Turtle per person | `/photos/people/kathy.ttl` |
| Locations | One Turtle per location | `/photos/locations/san-jose.ttl` |

**Cross-graph joins** (ADR-008 pattern):
```sparql
# Find all photos of Kathy in the garden
SELECT ?photo ?dateTaken ?albumTitle WHERE {
    GRAPH ?g1 {
        ?photo a jb:Photo ;
               jb:dateTaken ?dateTaken ;
               jb:hasPerson ?person ;
               jb:inAlbum ?album .
    }
    GRAPH ?g2 {
        ?person jb:personName "Kathy" .
    }
    GRAPH ?g3 {
        ?album jb:albumTitle ?albumTitle .
    }
}
ORDER BY DESC(?dateTaken)
```

---

## Thumbnail Strategy

- **Extract from Apple Photos** via JXA at 200x200 JPEG (same as music artwork)
- **Store as files** at `public/thumbnails/photos/{year}/{month}/{filename}.jpg`
- **Reference via URI** in Turtle: `jb:thumbnailPath "/thumbnails/photos/2024/06/rose.jpg"`
- **NOT base64 in RDF** — too bloated, kills Fuseki performance
- **Disk budget**: 50k photos x ~30KB = ~1.5GB. Acceptable per C2 (we'll have ~800GB free)

---

## Deduplication

**Primary key**: `photoFilename + dateTaken`

**Why**: Content hashes are expensive to compute for 50k+ photos. Filename + date taken uniquely identifies a photo in Apple Photos even across devices (iCloud sync normalizes these). If two files have the same name and same timestamp, they're the same photo.

**Fallback**: If filename is missing or generic (e.g., `IMG_0001.jpg`), use `dateTaken + imageWidth + imageHeight + cameraDevice` as composite key.

**Log duplicates**: Same pattern as Music — count skipped, report in harvest summary.

---

## What NOT to Ingest

- Full-resolution images (stay on disk, thumbnails only)
- RAW files
- iCloud-only photos (not downloaded locally)
- Deleted/trash items
- Photo editing history / versions

---

## Harvester Pattern

Reuse the Music harvester architecture:

1. **Extract**: JXA/AppleScript to read Apple Photos library metadata
2. **Transform**: Normalize names, generate URIs, build Turtle triples
3. **Deduplicate**: Check existing pods before writing
4. **Write**: Turtle files to pod directory (Pattern B)
5. **Sync**: Load into Fuseki named graphs
6. **Thumbnails**: Extract separately via JXA, write to public directory

**JXA access**: Apple Photos uses `Application("Photos")` in JXA. Key properties: `mediaItems`, `albums`, `containers`. Each `mediaItem` has: `id`, `name`, `filename`, `date`, `location` (lat/lon), `altitude`, `width`, `height`, `favorite`, `keywords`, `persons` (face tags).

---

## Cross-Domain Connections (Future)

Photos is the most connected domain. Future work (not in scope for v1):

| Connection | How |
|---|---|
| Garden photos → Garden domain | Match album names or GPS coordinates to garden beds |
| Property photos → Property domain | Match to rooms/house via album or location |
| People → Family/social graph | PhotoPerson becomes a cross-domain identity |
| Places → Travel/spatial dimension | PhotoLocation becomes reusable across domains |
| Time → Timeline view | dateTaken creates a temporal axis across all domains |

These connections are why Photos is strategically important — it's the bridge domain.

---

## Disk Impact Estimate (C7)

| Item | Estimate |
|---|---|
| Turtle files (50k photos metadata) | ~80-120 MB |
| Fuseki indexing overhead | ~50 MB |
| Thumbnails (50k x 30KB) | ~1.5 GB |
| **Total** | **~1.7 GB** |

With ~800GB free after Jeff's cleanup, this is well within budget.

---

— Silas
