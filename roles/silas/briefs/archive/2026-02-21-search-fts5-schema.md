# FTS5 Search Schema + Extraction Pipeline

**From:** Kade
**To:** Silas (review for sync safety)
**Date:** 2026-02-21
**Card:** #115 — Cross-collection search spike
**Source:** Clearing session 2026-02-21T18:14

## Decision Summary (from Clearing)

- **Option B: SQLite FTS5, pod-local**
- Explicit annotations only (no EXIF auto-index)
- Collection-level visibility
- Write-time indexing
- Derived view (rebuilt on sync)
- Photos collection first, expand to other collections later

## SQLite Schema

### Base table: `search_items`

```sql
CREATE TABLE IF NOT EXISTS search_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pod_id TEXT NOT NULL,              -- 'jeff', 'shared', etc.
    collection TEXT NOT NULL,          -- 'photos', 'books', 'music', etc.
    item_uri TEXT NOT NULL UNIQUE,     -- RDF URI: /photos/items/2024/01/sunset-beach
    item_type TEXT NOT NULL,           -- 'Photo', 'Book', 'MusicAlbum', etc.
    title TEXT NOT NULL,               -- dcterms:title
    description TEXT,                  -- jb:description (user-provided)
    tags TEXT,                         -- space-separated keywords from annotations
    album_or_group TEXT,               -- album title / shelf / playlist name
    collection_visibility TEXT NOT NULL DEFAULT 'private',  -- from .meta.ttl
    indexed_at TEXT NOT NULL           -- ISO 8601
);
```

### FTS5 virtual table: `search_fts`

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    title,
    description,
    tags,
    album_or_group,
    content='search_items',
    content_rowid='id'
);
```

Contentless FTS5 — delegates storage to `search_items`, indexed via triggers. Same pattern as Chorus context service.

### Sync triggers

```sql
CREATE TRIGGER IF NOT EXISTS search_ai AFTER INSERT ON search_items BEGIN
    INSERT INTO search_fts(rowid, title, description, tags, album_or_group)
    VALUES (new.id, new.title, new.description, new.tags, new.album_or_group);
END;

CREATE TRIGGER IF NOT EXISTS search_ad AFTER DELETE ON search_items BEGIN
    INSERT INTO search_fts(search_fts, rowid, title, description, tags, album_or_group)
    VALUES ('delete', old.id, old.title, old.description, old.tags, old.album_or_group);
END;

CREATE TRIGGER IF NOT EXISTS search_au AFTER UPDATE ON search_items BEGIN
    INSERT INTO search_fts(search_fts, rowid, title, description, tags, album_or_group)
    VALUES ('delete', old.id, old.title, old.description, old.tags, old.album_or_group);
    INSERT INTO search_fts(rowid, title, description, tags, album_or_group)
    VALUES (new.id, new.title, new.description, new.tags, new.album_or_group);
END;
```

### Performance indexes

```sql
CREATE INDEX IF NOT EXISTS idx_search_collection ON search_items(collection);
CREATE INDEX IF NOT EXISTS idx_search_pod ON search_items(pod_id);
CREATE INDEX IF NOT EXISTS idx_search_visibility ON search_items(collection_visibility);
CREATE INDEX IF NOT EXISTS idx_search_uri ON search_items(item_uri);
```

## Extraction Contract

### What constitutes an "annotation"

An annotation is any user-provided or harvested text metadata on an item:

| Field | Source | Example |
|-------|--------|---------|
| `title` | `dcterms:title` from Turtle | "Sunset at Crane Beach" |
| `description` | `jb:description` from Turtle | "Family trip, July 2024" |
| `tags` | `keywords[]` from PhotoItemRaw | "beach sunset vacation" |
| `album_or_group` | Album title from `jb:inAlbum` | "Summer 2024" |

**Not indexed (explicit scope):** EXIF data (camera, GPS, aperture), face detection metadata, harvest provenance, file paths, dimensions. These are structured queries via SPARQL, not full-text.

### When indexing triggers

**Write-time:** When any of these operations occur:

1. **Photo harvest completes** — bulk index all new/updated photos
2. **Tag/description edit via UI** — single item reindex
3. **Album membership change** — affected items reindexed
4. **Collection visibility change** — update `collection_visibility` on all items in that collection

### Indexing flow (photos)

```
PhotoHarvesterService.ingest()
  → writes Turtle to pod (existing flow)
  → calls SearchIndexService.indexPhotoBatch(photos[])
    → for each photo:
        extract title, description, keywords, album titles
        UPSERT into search_items (ON CONFLICT(item_uri) DO UPDATE)
        triggers auto-update search_fts
```

## Permission Post-Filter

### Query-time contract

```sql
-- User searches for "beach"
SELECT si.item_uri, si.title, si.description, si.collection, si.item_type,
       highlight(search_fts, 0, '<mark>', '</mark>') as title_hl,
       highlight(search_fts, 1, '<mark>', '</mark>') as desc_hl
FROM search_fts f
JOIN search_items si ON f.rowid = si.id
WHERE search_fts MATCH ?
  AND si.collection_visibility IN (?)  -- injected based on user's access level
ORDER BY rank
LIMIT 50;
```

### Visibility resolution

| User role | Sees collections with visibility |
|-----------|----------------------------------|
| admin | private, internal, public |
| member | internal, public |
| anonymous | public |

Resolution: Read user role from session → build visibility IN clause → pass to query. No permission logic in the index itself.

### Collection visibility source

Read from `.meta.ttl` per existing `collection-visibility.middleware.ts`. The middleware already caches this. The search service reads from the same source on index and stores the snapshot in `collection_visibility`. When visibility changes, the middleware invalidates its cache AND the search service bulk-updates affected rows.

## Sync Rebuild Trigger

### Derived view principle

The FTS5 index is never a source of truth. It's a computed artifact derived from pod Turtle files. On any full sync:

```
SearchIndexService.rebuildCollection(podId, collection)
  → DELETE FROM search_items WHERE pod_id = ? AND collection = ?
  → Read all Turtle files for that collection from pod
  → Extract annotations
  → Bulk INSERT into search_items
  → Triggers auto-populate search_fts
```

### When rebuild fires

1. **Full Fuseki sync** (`fusekiSyncService.fullSync()`) — the existing startup sync. After pod sync completes, trigger search reindex for the same pod.
2. **Manual harvest** — harvest endpoint already triggers Fuseki sync. Chain search reindex after.
3. **Explicit rebuild** — admin API endpoint: `POST /api/search/rebuild`

### Rebuild performance

- Current: ~13,000 photos, ~5,800 music albums, ~200 books
- SQLite bulk INSERT: ~50,000 rows/sec on M1
- Expected rebuild time: <1 second per collection

## Edge Cases

### Deleted items

When a Turtle file is removed or an item is removed from a Turtle file, the next rebuild pass catches it (DELETE + re-INSERT pattern). Between rebuilds, a deleted item might appear in search results but the link would 404 — acceptable for the spike. Follow-up: add delete triggers to harvester services.

### Collection renames

Collections don't rename (they're fixed: photos, books, music, etc.). Albums within collections might rename — handled by `item_uri` as the unique key (album slug is stable).

### Tag conflicts

Tags are space-separated text, not structured. No conflict resolution needed — last write wins. If two concurrent writes update the same item's tags, the later one overwrites. Acceptable at current scale (single user).

### Album renames

Album slug is derived from title at creation time and stays stable. If an album title changes, the `album_or_group` field updates on next reindex, but the URI stays the same. No orphan risk.

## File Structure

```
src/services/search-index.service.ts   -- SQLite schema, CRUD, rebuild, query
src/handlers/search.handler.ts         -- GET /api/search?q=...&collection=...
views/partials/search-bar.ejs          -- Reusable search component
```

Database location: `data/search.db` (same pattern as session store — `data/` is the app's SQLite directory, bind-mounted in Docker).

## Open Questions for Silas

1. **Sync determinism:** Rebuild-on-sync means the index is always consistent with pod state after sync. Any edge case where sync produces a different Turtle set on different runs?
2. **Reindex during sync:** If a search query hits while a rebuild is in progress (DELETE + INSERT), results will be temporarily incomplete. Worth adding a lock, or is the window too small to matter?
3. **Cross-pod scope:** For the spike, we index only `jeff` pod. When we add shared/public pods later, any structural concern with multiple pod_id values in the same index?
