# Photos Harvester: SQLite Pipeline + Multi-Source Person Architecture

**From**: Kade (Engineer)
**To**: Silas (Architect)
**Date**: 2026-02-17
**Re**: Replacing JXA extraction with SQLite, multi-source person detection model

---

## Discovery

While building Phase 2 (thumbnails) of the Photos Harvester, I explored the Apple Photos SQLite database at `~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite`. The findings are significant enough to warrant an architectural review.

---

## Finding 1: SQLite Replaces JXA for Extraction

**Current pipeline**: JXA `Application("Photos")` → stream JSON lines → parse → dedup → write Turtle
- Takes ~30 minutes for 9,684 photos
- JXA `item.location()` returns null for GPS (data loss)
- JXA doesn't expose face detection data

**Proposed pipeline**: SQLite read-only query → parse → dedup → write Turtle
- Completes in seconds (simple SQL query)
- Has GPS for 5,856 photos (60%) — JXA returned 0
- Has face detection for 13,300 faces across 5,274 photos
- Has face coordinates (center, size) and quality scores
- Has pre-rendered 256x256 JPEG face crops for top 20 clusters

**Key tables and relationships:**
```
ZASSET (9,729 rows)
  ├── ZUUID → maps to JXA mediaItem.id() prefix
  ├── ZFILENAME, ZDATECREATED, ZWIDTH, ZHEIGHT, ZFAVORITE
  ├── ZLATITUDE, ZLONGITUDE (real GPS — not exposed by JXA)
  └── ZKIND (0=photo, 1=video)

ZDETECTEDFACE (13,300 rows)
  ├── ZASSETFORFACE → ZASSET.Z_PK
  ├── ZPERSONFORFACE → ZPERSON.Z_PK
  ├── ZCENTERX, ZCENTERY, ZSIZE (normalized coordinates)
  ├── ZQUALITY, ZHASSMILE, ZGLASSESTYPE
  └── face attributes (age, gender, hair, expression, etc.)

ZPERSON (5,621 rows)
  ├── ZFULLNAME (currently NULL for all — Jeff hasn't named anyone)
  ├── ZFACECOUNT (largest cluster: 1,051 faces)
  ├── ZPERSONUUID
  └── ZAGETYPE, ZGENDERTYPE

ZFACECROP (20 rows with JPEG data)
  ├── ZPERSON → ZPERSON.Z_PK
  └── ZRESOURCEDATA → 256x256 JPEG blob (FFD8FFE0 magic bytes confirmed)

ZGENERICALBUM → Z_33ASSETS → ZASSET (album membership)

ZADDITIONALASSETATTRIBUTES (extended metadata)
```

**Date conversion**: Apple Core Data timestamps (seconds since 2001-01-01):
```sql
datetime(ZDATECREATED + 978307200, 'unixepoch') -- → ISO 8601
```

**JXA ID mapping**: `ZASSET.ZUUID` = the UUID prefix of JXA's `mediaItem.id()` (confirmed: `3E5FD41B-...` matches).

**Risk**: Apple can change the SQLite schema between macOS versions. For a local-first personal tool, this is acceptable. We can version-detect and adapt.

**Architecture question**: Should the SQLite extraction be a new service (`PhotoSqliteService`) or replace the JXA extraction inline in `PhotoHarvesterService`? I recommend a new service — clean separation, and we keep JXA as a fallback.

---

## Finding 2: JXA Still Needed for Thumbnails

SQLite gives us metadata but NOT the actual photo files. For thumbnails, we still need:
- `Photos.export([item])` via JXA to get the full photo
- `sharp` to resize to 200x200

Performance: ~2 seconds per photo via JXA export. For 7,974 photos = ~4.5 hours.

I've already refactored the thumbnail pipeline to process in batches of 50 (peak disk ~1GB per batch instead of 150GB all-at-once). This runs overnight.

**Architecture question**: Is there a better approach for thumbnail generation that avoids JXA's per-photo export overhead? The Photos Library filesystem (`originals/` and `resources/derivatives/`) has the files but the path structure is opaque and version-dependent.

---

## Finding 3: Multi-Source Person Model

Jeff has a larger Google Photos library where he has **actually named people**. Apple Photos has 5,621 face clusters but 0 named. This means person detection needs to be multi-source:

**Data sources for persons:**
| Source | Named Persons | Face Detection | Photos |
|--------|--------------|----------------|--------|
| Apple Photos (SQLite) | 0 | 13,300 faces, 5,621 clusters | 9,729 |
| Google Photos (Takeout) | Yes (Jeff has labeled) | Yes | Larger library |

**Google Photos API limitation**: The Library API does NOT expose face/person data (Google removed it). Person data comes from **Google Takeout**, which exports JSON metadata per photo including `"people": [{"name": "..."}]`.

**Proposed person model:**
```turtle
# A person exists once, referenced from multiple sources
:jeff-bridwell a foaf:Person, jb:PhotoPerson ;
    foaf:name "Jeff Bridwell" ;
    jb:personSource <google-photos>, <apple-photos> ;
    jb:applePhotosClusterId "6972" ;       # Apple's anonymous cluster
    jb:googlePhotosPersonId "..." .        # Google's named person

# Photos from either source link to the same person
:photo-apple-123 jb:depictsPerson :jeff-bridwell .
:photo-google-456 jb:depictsPerson :jeff-bridwell .
```

**Architecture questions:**
1. Should `PhotoPerson` be a separate ontology class, or reuse `foaf:Person` with photo-specific properties?
2. How do we handle cross-source person merging? (Same person in Apple and Google)
3. Does the existing ontology v0.8.0 need revision for multi-source persons?
4. Should person data live in `photos/persons/{slug}.ttl` or a separate `persons/` container (since persons span Photos and potentially Music artists)?

---

## Finding 4: GPS Locations Available

5,856 photos (60%) have GPS coordinates in the SQLite database. Top location clusters:

| Location | Lat/Lng | Photos |
|----------|---------|--------|
| Boston/Roslindale area | 42.29/-71.14 | 2,068 |
| St. Louis area | 38.62/-90.21 | 278 |
| Suburban Boston | 42.29/-71.49 | 232 |
| East Boston | 42.29/-71.13 | 188 |
| St. Louis west | 38.63/-90.29 | 172 |
| Cambridge/Somerville | 42.31/-71.11 | 162 |
| Downtown Boston | 42.35/-71.08 | 101 |
| Mexico (San Miguel?) | 20.91/-100.75 | 74 |
| Copenhagen | 55.69/12.58 | 74 |

**Architecture question**: The ontology v0.8.0 has `PhotoLocation` with slug based on lat/lng. Should we use reverse geocoding for human-readable names, or keep lat/lng-based slugs? Reverse geocoding requires an API call (Nominatim/OpenStreetMap or Google Geocoding).

---

## Proposed Architecture Changes

1. **New service**: `PhotoSqliteService` — read-only queries against Photos.sqlite for metadata + face data
2. **Revised harvest pipeline**: SQLite for extraction (seconds) → JXA for thumbnails only (overnight batch)
3. **New service**: `GoogleTakeoutService` — parse Takeout JSON for named persons + photo metadata
4. **Cross-source person model**: `foaf:Person` with multi-source references
5. **GPS in ontology**: `PhotoLocation` populated from SQLite coordinates

**Need from you**: Architecture review of the multi-source person model and ontology implications. Does v0.8.0 need a revision, or can person support be additive?

---

## Summary

| Change | Impact | Effort |
|--------|--------|--------|
| SQLite extraction (replace JXA) | 1000x faster, GPS data, face data | Medium (new service) |
| Google Takeout person import | Named persons (the real value) | Medium (new service + parser) |
| Cross-source person model | Unified person identity | Needs architecture review |
| GPS location support | Map view, location browsing | Low (data already available) |
| Batched thumbnails (already done) | Runs overnight, disk-safe | Done |
