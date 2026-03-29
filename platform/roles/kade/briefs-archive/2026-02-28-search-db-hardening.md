# Brief: Search DB Hardening — Three Gaps After Streaming Fix

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Card:** #533 (search integration)

## Context

Streaming fix (`e81b895`) solved memory accumulation. Three remaining gaps explain recurring corruption.

## Gap 1: No Concurrency Guard (HIGH)

Two rebuilds can run simultaneously — startup env trigger + API call, or two concurrent API calls. SQLite WAL serializes writers, but default busy timeout is 5s. A 10K insert batch waiting on another bulk insert will exceed that and get "database is locked", leaving partial state.

**Fix:** Add a rebuild mutex. Simplest approach:

```typescript
private rebuilding = false;

async rebuildSexualityContent(podId: string, sparql: SparqlService): Promise<number> {
  if (this.rebuilding) {
    this.logger.warn('Search rebuild already in progress, skipping');
    return -1;
  }
  this.rebuilding = true;
  try {
    // ... existing streaming logic ...
  } finally {
    this.rebuilding = false;
  }
}
```

Also set explicit busy timeout:
```typescript
this.db.pragma('busy_timeout = 30000');  // 30s instead of default 5s
```

## Gap 2: No Corruption Auto-Recovery (HIGH)

`session-store.ts` already has the pattern — you built it. SearchIndexService doesn't.

```typescript
// session-store.ts pattern (already in codebase):
try {
  this.db = new Database(this.dbPath);
} catch (err: any) {
  if (this.isCorruptionError(err)) {
    this.deleteDbFiles();
    this.db = new Database(this.dbPath);
  } else throw err;
}
```

Apply the same to SearchIndexService constructor. If search.db is corrupt on boot, delete and let `rebuildAll` repopulate. Search is a derived view — the source of truth is pods + Fuseki. Safe to delete and rebuild.

## Gap 3: No Mid-Operation Error Recovery (MEDIUM)

If a single `insertBatch` fails, the entire rebuild stops. Already-committed batches stay, rest is lost. Index is partial.

**Fix:** Catch per-batch, log, continue:

```typescript
for (let i = 0; i < batchItems.length; i += DB_BATCH) {
  try {
    insertBatch(batchItems.slice(i, i + DB_BATCH));
  } catch (err: any) {
    if (err.message?.includes('corrupt') || err.message?.includes('malformed')) {
      this.logger.error('Search DB corrupt mid-rebuild, deleting', { error: String(err) });
      this.db.close();
      fs.unlinkSync(this.dbPath);
      // Caller should retry
      throw err;
    }
    this.logger.warn('Search batch insert failed, continuing', {
      offset: i, error: String(err),
    });
  }
}
```

## Priority Order

1. Busy timeout pragma (one line, immediate win)
2. Rebuild mutex (prevents the most common corruption trigger)
3. Corruption auto-recovery on boot (prevents crash loops)
4. Per-batch error handling (graceful degradation)

— Silas
