# Brief: sessions.db Auto-Recovery on Corruption

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-23
**Card:** #245 (WF-042)
**Priority:** P1 — this has broken Jeff's site 3 times in 2 sessions

## Problem

`data/sessions.db` keeps corrupting with "database disk image is malformed". It's happened 3 times today.

## Root Cause

The `data/` directory is bind-mounted into the Docker container. Both the **container** (app serving requests) and the **host** (test suite during git push) open `sessions.db` via `better-sqlite3`. SQLite's POSIX file locks don't propagate across Docker's macOS VM boundary, so WAL mode can't prevent concurrent writes. Result: corruption.

## Fix Required

In `src/services/session-store.ts`:

### 1. Add busy_timeout pragma (line ~31, after journal_mode)
```typescript
this.db.pragma('journal_mode = WAL');
this.db.pragma('busy_timeout = 5000');
```

### 2. Add auto-recovery on corruption

Wrap the constructor's database open in a try/catch. If the db is malformed, delete it and recreate:

```typescript
try {
  this.db = new Database(resolvedPath);
  this.db.pragma('journal_mode = WAL');
  this.db.pragma('busy_timeout = 5000');
} catch (err: any) {
  if (err.message?.includes('malformed') || err.code === 'SQLITE_CORRUPT') {
    // Sessions are ephemeral — safe to delete and recreate
    console.warn(`[session-store] Corrupted sessions.db detected, recreating: ${err.message}`);
    fs.unlinkSync(resolvedPath);
    try { fs.unlinkSync(resolvedPath + '-wal'); } catch {}
    try { fs.unlinkSync(resolvedPath + '-shm'); } catch {}
    this.db = new Database(resolvedPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('busy_timeout = 5000');
  } else {
    throw err;
  }
}
```

### 3. Add recovery to get/set/touch/destroy methods

Each method should catch malformed errors and self-heal:

```typescript
get(sid: string, callback: (err: any, session?: session.SessionData | null) => void): void {
  try {
    const row = this.getStmt.get(sid, Date.now()) as any;
    // ... existing logic
  } catch (err: any) {
    if (err.message?.includes('malformed')) {
      this.recover();
      callback(null, null); // lost session = re-login
    } else {
      callback(err);
    }
  }
}
```

With a shared `recover()` method that deletes + recreates the db and re-prepares statements.

### 4. Log the recovery event

So we can track frequency:
```typescript
console.warn(`[session-store] Auto-recovered from corruption at ${new Date().toISOString()}`);
```

## What NOT to do

- Don't move sessions.db out of the bind mount — other data files need the mount
- Don't switch to in-memory sessions — we want sessions to survive container restarts
- Don't try to fix Docker bind mount locking — that's a Docker/macOS limitation

## Testing

The existing `tests/unit/services/session-store.test.ts` uses temp files (good — doesn't hit production db). Add a test that:
1. Creates a sessions.db
2. Corrupts it (write garbage bytes)
3. Verifies the store auto-recovers and serves new sessions

## Impact

This turns a 500 error (site down, Jeff can't access anything) into a transparent recovery (sessions cleared, Jeff re-logs in). The fix is ~30 lines.

---
*Silas | Architect*
