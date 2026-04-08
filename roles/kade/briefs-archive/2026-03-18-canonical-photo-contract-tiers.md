# Brief: Wire SHACL Severity Tiers into Mapper and Harvest Pipeline

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-18
**Re:** #1507, #1505

## Issue

The canonical photo contract has no required/optional enforcement. ADR-010 defined three-tier SHACL validation (lines 63-170) but it was never implemented. Jeff flagged this yesterday — it caused confusion about which fields matter.

## What to Build

### 1. Semantic Mapper — add severity column

The mapper page (`/harvesting/mapper`) should show a **Required / Expected / Optional** badge per Consumer ICD field, matching ADR-010 tiers:

| Consumer Field | Tier | SHACL Severity |
|---------------|------|---------------|
| canonicalId | Required | `sh:Violation` |
| dateTaken | Required | `sh:Violation` |
| filename | Required | `sh:Violation` |
| source | Required | `sh:Violation` |
| servingPath (at least one) | Required | `sh:Violation` |
| width/height | Expected | `sh:Warning` |
| mediaType | Expected | `sh:Warning` |
| latitude/longitude (range) | Expected | `sh:Warning` (value validation is `sh:Violation` per ADR-010) |
| description | Optional | `sh:Info` |
| sceneLabels | Optional | `sh:Info` |
| personNames | Optional | `sh:Info` |
| albums | Optional | `sh:Info` |
| isFavorite | Optional | `sh:Info` |
| faceDetections | Optional | `sh:Info` |

**Note on lat/long:** ADR-010 has range validation as `sh:Violation` (must be -90/90 and -180/180 if present), but presence is `sh:Info`. Two separate checks.

### 2. Harvest pipeline — reject on Violation

Before writing records to the graph, validate against the tier. Records missing any `sh:Violation` field → reject, log, count. Records missing `sh:Warning` fields → accept, flag. This is Stage 4 of the ADR-010 pipeline.

## Reference

- ADR-010: `data/about/ADR-010-generalized-harvest-pipeline.md` (full SHACL shape example at line 96)
- DEC-095: mapper before harvest
- Wren is getting a parallel brief on the product side
