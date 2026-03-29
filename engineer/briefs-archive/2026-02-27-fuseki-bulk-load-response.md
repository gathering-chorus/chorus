# Response: Fuseki Bulk Load

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Card:** #504
**Date:** 2026-02-27

## Recommendation: Option 1 now, Option 4 later

### Immediate fix — parallel curls

Fuseki TDB2 uses MRSW locking — concurrent PUTs serialize at the write lock but HTTP/network overhead parallelizes. Safe to run 8-10 in parallel.

In `harvest-sync-fuseki.sh`, replace the sequential loop with:

```bash
find "$POD_DIR" -name '*.ttl' -print0 | xargs -0 -P8 -I{} \
  curl -s -o /dev/null -w "%{http_code} {}\n" \
  -u "admin:$FUSEKI_PW" \
  -X PUT \
  -H "Content-Type: text/turtle" \
  --data-binary @{} \
  "http://localhost:3031/pods/data?graph=$(graph_uri_from_path {})"
```

The `graph_uri_from_path` function already exists in the script — just needs to be wired into xargs. Notes (823 files) should drop from ~14 min to ~2 min.

### No concerns about hammering Fuseki

- TDB2 write lock is per-dataset, not per-graph. Parallel PUTs queue at the lock — no corruption risk.
- Memory is fine for TTL files this size (individual notes are tiny).
- Music (19K files) will be slower — consider bumping to `-P12` there and watching memory via Grafana during the run.

### Don't use Option 3 (tdb2.tdbloader)

Requires Fuseki offline. Not viable for live operations. Only useful for cold initial loads.

### Option 4 (app-side sync) is the eventual path

`FusekiSyncService.fullSync()` already does batched parallel with backpressure (3 concurrent, 200ms delay). Exposing it as a CLI trigger (`POST /api/harvest/sync?domain=notes`) would give you a one-command sync with built-in rate limiting. But that's a TypeScript change + deploy — do it when you need it, not now.

### Option 2 (batch SPARQL UPDATE) — skip it

TTL escaping inside `INSERT DATA {}` blocks is fragile. One bad literal and the whole batch fails. Not worth the complexity when parallel curls work fine.
