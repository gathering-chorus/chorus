# NiFi Photos Pipeline — Flow Specification

**Card:** #1705 | **Author:** Silas | **Date:** 2026-03-26

## Design Principles

1. **Two stages only:** Ingest → Canonical. No separate enrichment.
2. **A record is not canonical until it has date + thumbnail.** Incomplete = still in ingest.
3. **SHACL gate before Fuseki write.** `photo-shape.ttl` enforces completeness.
4. **Idempotent.** Run 10 times, same result. No duplicates, no data loss.
5. **Apple ZUUID is the canonical UUID** for Apple-sourced photos. Synthetic UUID only for non-Apple records.

## Infrastructure

- **NiFi:** Bedroom Mac (192.168.86.242:8443), creds: env NIFI_USER / NIFI_PASS
- **Fuseki:** Library Mac (localhost:3030), dataset: /pods
- **Canonical graph:** `urn:gathering:photos/canonical`
- **Thumbnail output:** `/Volumes/VideosNew/Gathering/Photos/generated/thumbnails/{YYYY-MM}/{uuid}.jpg` (CSC)
- **Apple Photos SQLite:** Library `~/Pictures/Photos Library.photoslibrary/database/Photos.sqlite`
- **iPhone backup SQLite:** Library `~/Library/Application Support/MobileSync/Backup/*/Manifest.db` + embedded Photos.sqlite
- **Takeout JSON:** Bedroom `/Volumes/VideosNew/Gathering/Photos/source/google-takeout/`

## Stage 1: Ingest (NiFi Process Group)

### 1a. Extract — Apple Photos (Library)

```
Processor: ExecuteSQL (JDBC to Photos.sqlite on Library via SSH tunnel or direct)
Query:
  SELECT a.ZUUID, b.ZORIGINALFILENAME,
         datetime(a.ZDATECREATED + 978307200, 'unixepoch') as dateTaken,
         a.ZLATITUDE, a.ZLONGITUDE,
         a.ZPIXELWIDTH, a.ZPIXELHEIGHT,
         a.ZKIND  -- 0=photo, 1=video
  FROM ZASSET a
  JOIN ZADDITIONALASSETATTRIBUTES b ON b.ZASSET = a.Z_PK
  WHERE a.ZTRASHEDSTATE = 0 AND b.ZORIGINALFILENAME IS NOT NULL

Output: FlowFile per record
  uuid = ZUUID (Apple's native UUID — this IS the canonical UUID)
  filename = ZORIGINALFILENAME
  dateTaken = ZDATECREATED converted to ISO 8601
  source = "apple-photos"
  appleUuid = ZUUID (same as uuid for this source)
```

### 1b. Extract — iPhone Backup (Library)

```
Processor: ExecuteSQL (JDBC to iPhone backup Photos.sqlite)
Query: same as Apple Photos but from backup copy
  Key difference: backup has multiple DCIM folders with reused filenames

Output: FlowFile per record
  uuid = md5("iphone" + "|" + filename + "|" + dateTaken)  -- synthetic
  filename = ZORIGINALFILENAME
  dateTaken = ZDATECREATED converted to ISO 8601
  source = "iphone"
  appleUuid = ZUUID from backup (for derivative lookup attempt)

Dedup: (filename, dateTaken ±2sec) per ICD constraint
  iPhone counter resets — IMG_0519.HEIC appears multiple times with different dates.
  Each is a DIFFERENT photo. Keep all, dedup only exact matches.
```

### 1c. Extract — Google Takeout (Bedroom)

```
Processor: ListFile + FetchFile (Bedroom local)
Path: /Volumes/VideosNew/Gathering/Photos/source/google-takeout/
  Parse: JSON sidecar files for dateTaken (photoTakenTime.timestamp)

Output: FlowFile per record
  uuid = md5("google-takeout" + "|" + filename + "|" + dateTaken)
  filename = from JSON
  dateTaken = photoTakenTime.timestamp → ISO 8601
  source = "google-takeout"

Gate: reject dateTaken within 7 days of known harvest dates (Takeout date fallback bug, ~15%)
```

### 1d. Enrich — Thumbnail Generation

```
Processor: ExecuteScript (Python) on each FlowFile

Resolution order:
  1. Pre-generated file at {CSC_THUMBS}/{bucket}/{uuid}.jpg — use it
  2. Apple derivative: derivatives/{appleUuid[0]}/{appleUuid}_4_5005_c.jpeg — sips resize to 400x300
  3. Apple original: originals/{appleUuid[0]}/{appleUuid}.{ext} — sips convert+resize
  4. Takeout source file: direct from extracted path — sips convert+resize
  5. FAIL → route to error queue (iCloud-only, no local file)

Output: thumbnailPath attribute set on FlowFile
  Value: /thumbnails/photos/{bucket}/{uuid}.jpg (relative to app public/)

Thumbnail file written to:
  /Volumes/VideosNew/Gathering/Photos/generated/thumbnails/{bucket}/{uuid}.jpg

Named by CANONICAL UUID — same UUID the handler will look up.
```

### 1e. Validate — SHACL Gate

```
Processor: ExecuteScript (Python, rdflib + pyshacl)

Input: FlowFile with all attributes
Action: construct RDF for this record, validate against photo-shape.ttl
Required: filename, dateTaken, source, thumbnailPath, uuid

PASS → route to Stage 2
FAIL → route to error queue with validation report
```

## Stage 2: Canonical (Fuseki Write)

```
Processor: InvokeHTTP (POST to Fuseki)
Target: http://192.168.86.36:3030/pods/data?graph=urn:gathering:photos/canonical
Method: SPARQL UPDATE — INSERT DATA per record

Idempotency: DELETE existing triples for this UUID before INSERT.
  DELETE WHERE { <urn:jb:photos/{uuid}> ?p ?o }
  INSERT DATA { <urn:jb:photos/{uuid}> ... }

This ensures re-running the pipeline replaces, not duplicates.
```

## Error Queue

Records that fail SHACL validation or thumbnail generation route here.
- Logged with reason (missing date, no local file, invalid source)
- Queryable: how many failed, why, from which source
- NOT written to canonical — invisible on the photos page
- Jeff can review error queue to decide: download from iCloud, re-extract, or accept the gap

## Idempotency Guarantees

1. Extract: same SQL query, same results (source data doesn't change between runs)
2. Dedup: (filename, dateTaken) key — same input produces same set
3. UUID: deterministic from inputs — same photo always gets same UUID
4. Thumbnail: check-before-generate — skip if file exists and >0 bytes
5. Fuseki write: DELETE+INSERT per UUID — replaces, never duplicates
6. Error queue: append-only log, not cumulative

## Handler Changes (Kade)

- `getAllPhotosFromCanonical`: read from Fuseki graph `urn:gathering:photos/canonical`
- Add `appleUuid` field to SPARQL SELECT for derivative endpoint fallback
- `resolveThumbnail`: check pre-gen thumbnail by canonical UUID first, then derivative by appleUuid
- Remove canonical-index.json dependency entirely
- Remove Google Takeout/Drive merge code (already disabled)

## Verification

After pipeline run:
1. Count canonical records in Fuseki — should be ~80K (minus SHACL failures)
2. Count thumbnails on disk — should match canonical count
3. Load /photos page 1, 5, 10, 20 — thumbnails on every tile
4. Spot-check 10 random photos — thumbnail matches the actual photo
5. Run pipeline again — no count change, no new files
6. Jeff says "that looks right"
