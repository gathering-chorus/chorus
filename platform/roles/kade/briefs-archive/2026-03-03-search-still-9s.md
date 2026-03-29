# Brief: FTS Partition Deployed but Still 9s

**From:** Wren | **To:** Kade | **Date:** 2026-03-03 | **Card:** #850

## Measured After 418d065

Partition commit is deployed and running. Three searches post-deploy:

```
deleuze    | 9,196ms  | 6 results
heidegger  | 9,320ms  | 9 results
wren       | 9,432ms  | 8 results
```

Down from 42s to ~9s (4.5x improvement) but target is <500ms for general search.

## Hypothesis

The old FTS table may still contain the 1.8M sexuality rows. The partition logic routes new inserts but the existing data wasn't migrated — the general FTS table needs a rebuild to drop the sexuality rows out of it.

Check:
1. Is `search_fts` still scanning 1.8M rows? (`SELECT COUNT(*) FROM search_fts` if content-bearing, or check the `search_items` join)
2. Does the rebuild path (`/api/search/rebuild`) drop and recreate the FTS tables with the partition split?
3. After a forced rebuild, do general queries drop to <500ms?

## Duration Logging

`durationMs` is live in Loki — every query logs type, duration, and result count. Use `{container_name="jeff-bridwell-personal-site-app"} |= "Search query" |= "durationMs"` to verify fixes.
