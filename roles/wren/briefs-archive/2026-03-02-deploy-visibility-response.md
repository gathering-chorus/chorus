# Brief: Deploy Visibility + Session Persistence — Response

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-03-02
**Re:** Deploy Visibility + Session Persistence

## Problem #1: Deploy Logging — FIXED

The issue wasn't that `app-state.sh` was broken — it emits events fine. The problem was **roles bypassing `app-state.sh`** entirely. 27 commits yesterday, zero deploy events logged.

**Fix shipped:** Added auto-deploy to pre-push git hook. Now:
- `src/` or `package.json` changed → auto-deploy via `app-state.sh` in background (fully logged)
- Views/CSS only → skip deploy, log `deploy.skipped` event
- Every deploy is now visible in chorus log + Prometheus

## Problem #2: Session Persistence — ROOT CAUSE FOUND

We already have a SQLite-based session store (`session-store.ts`) on a bind-mounted volume. Sessions SHOULD survive deploys. But they don't because:

1. Docker `stop` sends SIGTERM → container has 10s to shut down
2. SQLite WAL file (1.9MB) hasn't been checkpointed
3. Container gets SIGKILL after timeout
4. On restart, SQLite recovery detects corruption in the WAL
5. `session-store.ts` auto-recovery **deletes** sessions.db and recreates empty
6. Jeff's session is gone → forced re-login → 28-second SOLID round-trip

Evidence: two `.corrupted` backup files from Feb 23 when this was first noticed.

**Fix options (for Kade to implement):**
- **A) Graceful shutdown handler** — listen for SIGTERM, run `db.pragma('wal_checkpoint(TRUNCATE)')` before exit. This checkpoints the WAL cleanly so restart doesn't hit corruption. ~10 lines of code. Low risk.
- **B) Periodic WAL checkpoint** — run `wal_checkpoint` every 5 minutes instead of relying on shutdown. Belt-and-suspenders.
- **C) Both** — recommended. A handles deploys, B handles crashes.

Recommend Kade cards this and ships it. Small change, big impact on Jeff's daily experience.
