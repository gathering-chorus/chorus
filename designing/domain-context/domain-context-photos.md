# Domain Context: Photos

Last updated: 2026-03-26 by Wren (#1688)

## ICD

| File | What it defines |
|------|----------------|
| `src/ontology/icd-instance-photos.ttl` | Source-to-canonical field mappings for Apple, Takeout, iPhone. Era-scoped authority rules (#1642). |
| `architect/docs/merge-specification-photos.html` | Merge spec: per-field precedence, era boundaries, conflict resolution, NiFi processor mapping. |
| `src/ontology/icd-ontology.ttl` | Era, EraAuthority, MetadataCeiling, MigrationEvent class definitions. |

## Tests

| File | Coverage |
|------|----------|
| `chorus/bridge/tests/nudge-integration.test.ts` | Nudge delivery pipeline — 30 tests |
| `messages/board-client/tests/nudge-pipeline-flow.test.ts` | Nudge flow + spine events — 18 tests |

No photos-specific unit tests yet. The canonical rebuild (#1644) needs validation tests against known 3-source matches (e.g., IMG_2774.HEIC across Apple/Takeout/iPhone).

## Persistence

| Type | Location | Details |
|------|----------|---------|
| Fuseki — Apple source | `urn:gathering:photos/source/apple` | 24,592 photos (221K triples) |
| Fuseki — Takeout source | `urn:gathering:photos/source/takeout` | 102,488 photos (944K triples) |
| Fuseki — iPhone source | `urn:gathering:photos/source/iphone` | 54,479 photos (533K triples) |
| Fuseki — Canonical | `urn:jb:photos/canonical/` | 79,598 photos (980K triples) |
| NiFi — Apple ingest | "Photo — Apple Photos Source Ingest" on Bedroom | DSL-created via `nifi-dsl.sh elt`, port 8878. 24K records. |
| NiFi — iPhone ingest | "Photo — iPhone Source Ingest" on Bedroom | DSL-created via `nifi-dsl.sh elt`, port 8877. 54K records. |
| NiFi — Takeout ingest | (not yet created) | Next: #1711 |
| Source files — Apple | `~/Pictures/Photos Library.photoslibrary/` on Library | 24K assets, SQLite |
| Source files — Takeout | `/Volumes/VideosNew/Gathering/Photos/source/google-takeout/` (CSC target) | 118K files on Bedroom |
| Source files — iPhone | `~/Library/Application Support/MobileSync/Backup/` on Library | Finder backup, 2.1GB SQLite |
| Thumbnails | `/Volumes/VideosNew/Gathering/Photos/generated/thumbnails/` (CSC) | 200x200 JPEG, date-bucketed |
| Scripts | `scripts/harvest-iphone-photos.sh` | iPhone SQLite → JSON extraction |
| Scripts | `engineer/scripts/build-nifi-photos-flow.py` | NiFi flow generator |
| Scripts | `jeff-bridwell-personal-site/scripts/nifi/photos-merge.py` | Merge logic (moved home #3599 — implementation-of-convergence stays in gathering; Groovy port at scripts/nifi-groovy/) |

## Key Decisions

| Decision | Summary |
|----------|---------|
| DEC-094 | Harvest pause — tighten operations before scaling data migration |
| DEC-095 | Mapper before harvest — no data loading without validated ICDs |
| Jeff 2026-03-24 | Canonical rebuild runs through NiFi, not bash/TypeScript |
| Jeff 2026-03-24 | iPhone extraction must be NiFi-native (JDBC), not bash wrapper |
| Jeff 2026-03-26 | All NiFi pipelines through DSL (`nifi-dsl.sh elt`). Framework before more pipelines (#1708). |
| Jeff 2026-03-26 | ICD-driven runtime SPARQL — Groovy reads field mappings from Fuseki at transform time. No hardcoded fields. Live governance verified. |
| Jeff 2026-03-26 | NiFi naming: noun-verb convention matching OAGIS (e.g., "Photo — iPhone Source Ingest"). |
| CSC (2026-03-25) | All source files under `/Volumes/Gathering/Photos/source/`, generated under `generated/`. Never `/tmp/`. |

## Constraints

**Read these before touching photos code. Each was learned the hard way.**

- **Apple Photos uses UUID filenames internally.** ZUUID is the identifier, not ZFILENAME. Don't match across sources by Apple's internal filename — it's meaningless outside Photos.sqlite.
- **iPhone reuses IMG_NNNN across years.** IMG_0001.HEIC from 2021 and 2024 are different photos. Dedup by filename + dateTaken (±2 sec), not filename alone. Filename-only matches with date divergence >30 days go to dead-letter.
- **Source priority shifts by era.** Apple is golden for Eras 1-3 (pre-2020). iPhone is golden for Era 4 (2020+). Takeout is supplementary/volume-fill all eras. See merge spec Section 1.
- **Era boundaries: 2006, 2013, 2020.** Pre-digital → Camera → iPhone Primary → Modern. The merge logic, authority chains, and metadata ceilings all depend on these dates.
- **Takeout date fallback bug.** ~15% of Takeout records have `photoTakenTime` set to the upload date (2026-03-12) instead of capture date. Gate: reject `dateTaken` within 7 days of any known harvest date.
- **Takeout JSON never has dimensions or fileSize.** Permanent format limitation (metadata ceiling). Don't try to fill these from Takeout — they don't exist.
- **Cross-graph SPARQL joins timeout on full datasets.** Use per-era CONSTRUCT queries with FILTER on dateTaken ranges, not one query for all 180K records.
- **NiFi binds to 192.168.86.242:8443, not localhost.** Use the machine IP for all NiFi API calls from either machine.
- **Canonical UUID function (pinned).** `uuid = md5(filename + "|" + dateTaken)` formatted as `{8}-{4}-{4}-{4}-{12}`. This is the ONLY UUID scheme. Thumbnail files are named `{bucket}/{uuid}.jpg`. The pipeline (`photo-pipeline.py`), index builder, and thumbnail generator all use `make_uuid(filename, date)`. Changing this function orphans all existing thumbnails — never change it without renaming all files on disk.
- **Thumbnail source resolution.** For apple-photos records: look up original filename in Photos.sqlite `ZADDITIONALASSETATTRIBUTES.ZORIGINALFILENAME` → get Apple UUID from `ZASSET.ZUUID` → copy derivative from `derivatives/{UUID[0]}/{UUID}_4_5005_c.jpeg` as `{bucket}/{canonical_uuid}.jpg`. For iphone-only records not in Photos Library: extract from iPhone backup via Manifest.db hash lookup.
- **Thumbnail generation must match by filename+date, not filename alone.** iPhone reuses `IMG_NNNN` across DCIM folders — same filename, different physical photos. `IMG_2928.HEIC` exists 6 times (2019→2025) in the backup's Photos.sqlite, each in a different `DCIM/1xxAPPLE/` folder. The generator must resolve the correct physical file using the ZASSET row that matches BOTH `ZFILENAME` and `ZDATECREATED`, then look up the file via `ZUUID` or `ZDIRECTORY`. Matching by filename alone produces visually wrong thumbnails (confirmed 2026-03-25).
- **Apple originals path changed.** SQLite ZDIRECTORY is stale. Actual path: `originals/{ZUUID[0]}/{ZUUID}.{ext}`, not `originals/{ZDIRECTORY}/{ZFILENAME}`.
- **Takeout source files on Bedroom.** Full path: `/Volumes/VideosNew/Gathering/Photos/GoogleTakeoutPhotos/extracted/Takeout/Google Photos/{year}/`. 118K files. The Google Drive path (3K files) is live sync, NOT the export.
- **ffmpeg on Bedroom.** Installed at `/opt/homebrew/bin/ffmpeg`. Not in SSH PATH — use full path in scripts.
- **CSC thumbnail path.** Output to `/Volumes/VideosNew/Gathering/Photos/generated/thumbnails/{YYYY-MM}/{uuid}.jpg` on Bedroom. Never `/tmp/`.
- **NiFi creds: admin / nifi-gathering-2026.** Reset 2026-03-24.
- **Fuseki graph URIs in the merge spec are aspirational.** Always verify actual graph URIs against Fuseki before building SPARQL queries. The spec says `urn:jb:photos/apple-source` but Fuseki may use different names.
- **Face clusters don't merge across iPhoto→Photos.app boundary (2015-04).** Pre-2015 face IDs are iPhoto-era, not continuous with post-2015 ML clusters.
- **Mac Photos library frozen since 2020.** iCloud sync disabled — only 975 records in Era 4. Apple is supplementary for Modern era, not golden.
