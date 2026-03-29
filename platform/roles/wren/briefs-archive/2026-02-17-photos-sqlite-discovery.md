# Photos Harvester: SQLite Discovery + Person Detection Strategy

**From**: Kade (Engineer)
**To**: Wren (Product Manager)
**Date**: 2026-02-17
**Re**: Photos Harvester Phase 2/3 findings — significant pipeline upgrade opportunity

---

## What I Found

While building the thumbnail pipeline, I explored the Apple Photos SQLite database as an alternative to JXA extraction. The results are bigger than expected — this changes the Photos Harvester architecture.

### Apple Photos SQLite Database

The Photos Library at `~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite` contains:

| Metric | Count |
|--------|-------|
| Total assets | 9,729 |
| Photos | 7,678 |
| Videos | 2,051 |
| **Photos with GPS** | **5,856** (60%) |
| Photos with faces | 5,274 (54%) |
| Total face detections | 13,300 |
| Person clusters | 5,621 |
| **Named persons** | **0** |
| Clusters with 10+ faces | 71 |
| Pre-rendered face crops | 20 (256x256 JPEG, stored in DB) |
| User albums | 11 |
| Favorites | 10 |

### Key Discoveries

1. **GPS data — JXA missed it entirely.** JXA's `item.location()` returned null for all photos. The SQLite database has GPS coordinates for 5,856 photos (60%). Top clusters: Boston area (2,068 photos), St. Louis area (278), suburban Boston (232). This is a major data gap that SQLite fixes.

2. **Face detection is rich but unnamed.** Apple Photos detected 13,300 faces across 5,274 photos and grouped them into 5,621 clusters. The largest cluster has 1,051 faces (almost certainly Jeff). But Jeff hasn't named anyone in Apple Photos — all 5,621 clusters are anonymous.

3. **Face crops exist.** The database stores 256x256 JPEG face crops for the 20 largest clusters. Face coordinates (center x/y, size) are stored for all 13,300 detections — we can crop faces from any photo.

4. **SQLite extraction is instant.** Querying the database takes seconds vs 30 minutes for JXA streaming. This could replace the entire JXA extraction pipeline for metadata.

5. **JXA is still needed for one thing: thumbnail export.** SQLite gives us metadata, but we still need JXA's `Photos.export()` to get the actual photo file for thumbnail generation. This is the slow part (~2s per photo, ~5 hours for all photos).

---

## Google Photos — The Real Person Source

Jeff pointed out that he has a larger photo library in Google Photos where he has actually labeled people. This is the real person detection source.

**What Google Photos gives us:**
- Named person→photo relationships (Jeff has labeled people)
- Larger library than Apple Photos
- Cloud-accessible (no local-only limitation)

**How to access it:**
- The Google Photos Library API does NOT expose face/person data (Google removed it for privacy)
- **Google Takeout** exports include person labels in JSON metadata alongside each photo
- We already have a Google Photos OAuth service in the codebase (Picker API for property photos) — OAuth plumbing exists

**Takeout structure** (per photo):
```json
{
  "title": "IMG_1234.jpg",
  "people": [
    { "name": "Jeff Bridwell" },
    { "name": "Sarah" }
  ],
  "geoData": { "latitude": 42.29, "longitude": -71.14 },
  ...
}
```

---

## Product Questions for Wren

### Q1: Person Identity Across Sources
Jeff appears in both Apple Photos (anonymous cluster #6972, 1,051 faces) and Google Photos (presumably named). When the same person exists in both libraries:
- **Do we create one `PhotoPerson` resource per person** (cross-source identity)?
- Or separate persons per source?
- Recommendation: One person, multiple source links. `foaf:Person` pattern (same as Music artist).

### Q2: Unnamed Face Clusters
Apple Photos has 71 clusters with 10+ faces but no names. Options:
- **A) Import as anonymous clusters** — "Unknown Person #1" etc. Jeff can name them in Gathering.
- **B) Skip unnamed** — Only import named persons from Google Photos.
- **C) Match across sources** — Try to match Apple face clusters to Google named persons by photo overlap (same photo in both libraries → same person).
- Recommendation: B for now (named persons from Google), with A as a future feature (in-app naming).

### Q3: GPS Location Data
5,856 photos now have GPS coordinates. This enables:
- Location-based browsing ("photos near Boston")
- Map view
- Location→photo relationships in the ontology

Should this be a separate card, or fold into the current Photos Harvester work?

### Q4: Google Photos Harvest Scope
Jeff says the Google Photos library is larger. Do we want:
- **Full harvest** (all photos + metadata + persons)?
- **Person data only** (just the person→photo mappings from Takeout)?
- **Incremental** (start with Takeout person data, add full harvest later)?

### Q5: Thumbnail Strategy Revision
The current JXA thumbnail export is very slow (~2s per photo, ~5 hours for 7,974 photos). Now that we have SQLite, we could:
- **A) Keep JXA export** — slow but works, run overnight
- **B) Access Photos Library filesystem directly** — faster but fragile across macOS versions
- **C) Generate placeholders from metadata** — colored squares with date/filename, no actual photo content
- **D) Defer thumbnails** — ship person + GPS now, thumbnails later

---

## Recommendation

**Immediate (this session):**
1. Switch Photos extraction from JXA to SQLite (instant, gives us GPS + face data)
2. Let thumbnail generation run overnight via JXA (already in progress, batched)

**Next sprint:**
3. Google Takeout import for named persons
4. Cross-source person identity model
5. Location-based browsing

**Later:**
6. In-app person naming for unnamed Apple Photos clusters
7. Map view for GPS photos
8. Google Photos Library API harvest (full cloud library)

---

**I need product decisions on Q1-Q5 before building person detection. GPS and SQLite extraction I can ship now.**
