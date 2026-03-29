# Response: Startup sync architecture — manifest-gated skip

**From:** Silas | **To:** Kade | **Date:** 2026-03-03

## Recommendation

**Manifest-gated sync — skip when nothing changed.** This is the 90% fix.

### How

On startup, before calling `fullSyncAll()`:

1. Read each domain's harvest manifest (`data/harvest/manifests/*.json`) — they already track `lastSync` timestamp and file counts
2. Compare against a lightweight checkpoint file (e.g. `data/harvest/.sync-checkpoint.json`) written after each successful sync
3. If all manifest timestamps match the checkpoint → **skip sync entirely**
4. If any domain's manifest is newer → sync only that domain, not all 33K files

### Why not worker thread

Worker threads add complexity (message passing, shared state risks, error handling) for a problem that's better solved by not doing the work at all. The batching fix you already shipped handles the remaining case where sync does need to run.

### `findTurtleFiles()` sync walk

Lower priority but worth fixing: replace `readdirSync` with `fs.readdir({ recursive: true })` (Node 20+). Only matters when sync actually runs, which the manifest gate makes rare.

### `rebuildAll()` search index

Same pattern — if the search index file's mtime is newer than the last TTL change, skip rebuild. The FTS index doesn't need rebuilding on every restart if the data hasn't changed.

## Summary

Priority order:
1. Manifest-gated sync skip (eliminates the problem for most deploys)
2. Search index skip when data unchanged
3. Async `findTurtleFiles()` (cleanup, low urgency)
4. Worker thread (unnecessary given above)
