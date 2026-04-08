# Brief: Fuseki Bulk Load — harvest-sync-fuseki.sh is too slow

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Card:** #504
**Date:** 2026-02-27

## Problem

`harvest-sync-fuseki.sh` does one HTTP PUT per TTL file to Fuseki's GSP endpoint. At ~1 req/sec, 823 notes takes ~14 minutes. This blocks the manifest pipeline — `harvest run notes load` is unusable at this speed.

## Current approach

```bash
for each .ttl file:
  curl -X PUT /pods/data?graph=<uri> --data-binary @file.ttl
```

Sequential, one graph per request, full auth handshake each time.

## Options I see

1. **Parallel curls** — `xargs -P10` or GNU parallel. Quick fix, 5-10x speedup. Still N requests but concurrent.

2. **Batch SPARQL UPDATE** — concatenate TTL into `INSERT DATA { GRAPH <uri> { ... } }` blocks, POST to `/pods/update`. One request per batch. Needs careful escaping.

3. **Fuseki bulk loader** — `tdb2.tdbloader` or Fuseki's `/$/datasets/<name>/data` bulk endpoint. Fastest for initial loads. Requires knowing Fuseki's internal API.

4. **App-side fullSyncAll()** — the existing `FusekiSyncService.fullSync()` already does batched parallel (3 at a time with 200ms delay). Could expose a CLI trigger that calls the app's sync endpoint for a specific pod path.

## What I need from you

- Which approach fits the infra? You own Fuseki config.
- Is there a bulk load endpoint already configured?
- Any concerns about hammering Fuseki with parallel PUTs (memory, TDB2 locking)?

## Context

Jeff wants manifest-driven harvests that run fast enough to be interactive. Notes (823) and WordPress (42) are the proof points. Music (19K) and sexuality (2M) will need this even more.
