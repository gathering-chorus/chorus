# Photos Source Richness Scorecard

**Card:** #1633 | **Date:** 2026-03-23 | **Author:** Kade (builder), Wren (navigator)

## Purpose

Score every photo source against the Photos ICD schema before choosing the canonical anchor. Replaces the volume-based decision (Google Takeout: 99K records) with a richness-based decision.

Jeff context: Pre-2006 no digital camera. 2006-2007 dedicated camera. iPhone since Dec 2007 — nearly all photos are iPhone origin. Apple Photos is the authoritative source. Google Takeout is copies.

## ICD Field Inventory (20 fields)

| # | Field | Tier | Apple Photos | Google Takeout | Google Drive | osxphotos |
|---|---|---|---|---|---|---|
| 1 | canonicalId | high | ZUUID (computed) | — | — | — |
| 2 | servingPath | high | computed | — | — | — |
| 3 | thumbnailPath | high | computed | — | — | — |
| 4 | hasSourceRecord | high | ZUUID | — | id | — |
| 5 | filename | high | ZFILENAME | — | name | — |
| 6 | dateTaken | high | ZDATECREATED | photoTakenTime | — | date |
| 7 | dimensions | high | ZWIDTH/ZHEIGHT | — | — | — |
| 8 | location | medium | ZLATITUDE/ZLONGITUDE | geoData | — | location |
| 9 | mediaType | high | ZKIND | — | — | — |
| 10 | isFavorite | high | ZFAVORITE | favorited | — | — |
| 11 | fileHash | high | ZORIGINALSTABLEHASH | — | — | — |
| 12 | bestResPath | medium | ZDIRECTORY | — | — | — |
| 13 | faces | low | ZDETECTEDFACE | — | — | — |
| 14 | people | high | ZPERSON.ZFULLNAME | people | — | — |
| 15 | sceneLabels | high | — | — | — | labels |
| 16 | albums | medium | — | albumName | — | albums |
| 17 | description | low | — | description | — | — |
| 18 | deviceType | low | — | deviceType | — | — |
| 19 | fileSize | high | — | — | size | — |
| 20 | webViewLink | high | — | — | webViewLink | — |

## Richness Scores

| Source | Fields Covered | Coverage | High-Tier Coverage |
|---|---|---|---|
| **Apple Photos (native SQLite)** | **14/20** | **70%** | 11/13 high (85%) |
| Google Takeout Photos | 7/20 | 35% | 4/13 high (31%) |
| Google Drive (routed) | 4/20 | 20% | 4/13 high (31%) |
| osxphotos (supplemental) | 4/20 | 20% | 2/13 high (15%) |

## Recommendation

**Apple Photos SQLite as canonical anchor.**

Rationale:
1. **2x richer** than the next source (70% vs 35%)
2. **85% high-tier coverage** — the fields that matter most for search, serving, and dedup
3. **Authoritative source** — iPhone is the camera, Apple Photos is where originals live
4. **Unique capabilities**: file hashes (dedup), face detections, dimensions, media type, best-res path
5. **Computed fields** (canonicalId, servingPath, thumbnailPath) can only come from Apple Photos

Google Takeout as **enrichment layer** for: albums, description, deviceType, geoData (where Apple location is missing).

osxphotos as **enrichment layer** for: sceneLabels (ML-derived, no other source has this).

## Impact on Pipeline

This reverses the current architecture where Google Takeout's 99K records anchor canonical. Instead:
- Apple Photos 24.5K assets become the canonical set
- Google Takeout enriches with album membership and descriptions
- Face detections link directly (same UUID space — no reconciliation needed)
- The 48K canonical gap from #1625 disappears (those were Takeout records without Apple counterparts)

## Gates

This scorecard gates:
- #1627 (album import) — proceed, but as enrichment into Apple anchor, not Takeout anchor
- #1630 (embedding rebuild) — rebuild after re-anchoring
- Any new photo harvester work

## Data Sources Queried

- ICD: `src/ontology/icd-instance-photos.ttl` (29 FieldMappings, 6 Providers, 20 CanonicalFields)
- Apple Photos SQLite: `~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite` (24,592 assets)
- Fuseki: canonical graph `urn:jb:photos/canonical/` (62,998 photos), album graphs (190 albums)
