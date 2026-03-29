# Brief: Batch Photo Harvest Pipeline Architecture

**From:** Silas (Architect) → Kade (Engineer)
**Date:** 2026-03-12
**Card:** #1351

## Context

Jeff has 157 Google Takeout files, each ~4GB (~628GB total), landing on `/Volumes/Gathering/Photos/GoogleSnapshot`. Current harvest script handles one directory at a time with manual intervention. That won't scale.

## Requirements

1. **Resumable batch runner** — process files 1-157 sequentially. Track state in a manifest: `{file, status: pending|processing|done|failed, triples_loaded, timestamp}`. On restart, skip `done`, retry `failed` once, continue from first `pending`.

2. **Per-file checkpoint** — each file gets: extract → TTL → load to Fuseki → verify count → mark done. Crash at any step = that file stays `processing`, gets retried next run.

3. **Dedup** — Google Takeout exports overlap heavily (same photo in multiple albums). Dedup by photo URL or content hash before Fuseki ingest. Don't load duplicate triples.

4. **Disk management** — Keep all raw files until the full batch is processed and verified. No early deletion. Delete intermediate TTL after successful Fuseki load to save space. Budget: ~628GB raw + TTL churn on NFS (Bedroom has headroom).

5. **Sync integration** — #1347 must land first. New TTL files need to be picked up by the sync without manual checkpoint deletion.

6. **Overnight capability** — this should be runnable as a LaunchAgent. 157 files at ~10 min each = ~26 hours. Needs to survive machine sleep/wake and NFS disconnects gracefully.

## Arch Constraints

- All writes go through `PodWriteService` with correct constructor wiring (the bug you just fixed)
- Same `jb:Photo` ontology shape confirmed — no schema changes needed
- Watch Fuseki memory: 15.7M triples today, this could add 20-50M more. JVM heap may need tuning.
- Perf baseline (#1344) is running nightly — we'll see the impact automatically

## Recommendation

Build the batch runner as a standalone Node script (like `harvest-google-photos.js`) with a JSON manifest for state. Don't try to make the app's startup sync handle this — that's a different concern (#1347).
