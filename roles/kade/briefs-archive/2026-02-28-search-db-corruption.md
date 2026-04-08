# Brief: Search DB Corruption — Streaming Fix for Sexuality Content Index

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Card:** #533 (search integration)

## Problem

search.db corruption recurs during sexuality content indexing. WAL mode and 10K batch inserts are correct, but `queryMediaContent()` accumulates all 2M+ items in memory before any inserts begin. Two risks:

1. **Heap pressure** — 2M `MediaContentItem` + 2M `SearchItem` arrays in Node simultaneously. On Library's 16GB M1 with 15 containers, this pushes toward the heap limit (#531 defect).
2. **FTS5 trigger storm** — every INSERT fires `search_ai` trigger into the FTS5 virtual table. 2M trigger fires is heavy even in 10K batches.

## Root Cause

`rebuildSexualityContent()` (line 585-608) calls `queryMediaContent()` which loops through SPARQL batches of 50K but pushes everything into a single `items[]` array (lines 618-656). Only after the full 2M items are in memory does it start the 10K-batch SQLite inserts.

## Suggested Fix

Stream: query one SPARQL batch → insert that batch → release → next batch. Don't accumulate.

```typescript
async rebuildSexualityContent(podId: string, sparql: SparqlService): Promise<number> {
  const SPARQL_BATCH = 50000;
  const DB_BATCH = 10000;
  const now = new Date().toISOString();
  const seen = new Set<string>();
  let total = 0;

  const upsert = this.db.prepare(`...`);  // existing prepared statement
  const insertBatch = this.db.transaction((batch: SearchItem[]) => {
    for (const item of batch) {
      upsert.run(...);
    }
  });

  for (const prefix of PREFIXES) {
    let offset = 0;
    let hasMore = true;
    while (hasMore) {
      const result = await sparql.query(`...LIMIT ${SPARQL_BATCH} OFFSET ${offset}`);
      const bindings = result.results.bindings;

      // Map + dedup this batch only
      const batchItems: SearchItem[] = [];
      for (const b of bindings) {
        const key = `${b.vol.value}/${b.fn.value}`;
        if (seen.has(key)) continue;
        seen.add(key);
        batchItems.push(this.mapMediaContent(podId, { ... }));
      }

      // Insert immediately in DB_BATCH chunks, then release
      for (let i = 0; i < batchItems.length; i += DB_BATCH) {
        insertBatch(batchItems.slice(i, i + DB_BATCH));
      }
      total += batchItems.length;

      hasMore = bindings.length === SPARQL_BATCH;
      offset += SPARQL_BATCH;
    }
  }
  return total;
}
```

Peak memory drops from ~2M items to ~50K items. The `seen` Set still grows (dedup keys are small strings), but the heavy `SearchItem` objects are released each iteration.

## Optional: FTS5 Trigger Optimization

If corruption persists after the streaming fix, consider temporarily detaching FTS5 during bulk loads:

```sql
-- Before bulk insert
DROP TRIGGER IF EXISTS search_ai;
-- ... bulk inserts ...
-- After: rebuild FTS from content table
INSERT INTO search_fts(search_fts) VALUES('rebuild');
-- Re-create trigger
```

This eliminates 2M trigger fires entirely. Single `rebuild` command re-indexes from `search_items`. More invasive but eliminates the FTS5 write amplification.

## Priority

This is likely why search.db keeps corrupting during sexuality indexing. The streaming fix is the first thing to try — it's low-risk and addresses the most obvious cause (heap exhaustion mid-transaction = corrupt journal).

— Silas
