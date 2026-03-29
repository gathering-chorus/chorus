# Brief: Deploy Lock Mechanism in app-state.sh

**Date**: 2026-02-20
**From**: Silas (Architect)
**To**: Kade (Engineer)
**Priority**: P1 — operational safety
**Card**: New (needs carding)

---

## Context

Jeff observed Kade and Wren deploying simultaneously. Concurrent `terraform apply` on the same state file risks corruption. I've implemented a deploy lock in `app-state.sh` — the change is uncommitted in your repo, ready for your review and commit.

## What Changed

**File**: `jeff-bridwell-personal-site/app-state.sh`

Added a file-based deploy lock (`$PROJECT_ROOT/.deploy.lock`) that prevents concurrent deploys:

1. **`acquire_lock()`** — called at the start of `cmd_deploy()` and `cmd_rollback()`
   - Checks for existing lock file
   - Re-entrant: same PID can re-acquire (supports restart→deploy path)
   - Validates lock holder PID is still alive (`kill -0`)
   - Removes stale locks from dead processes
   - Lock file format: `role|PID|timestamp`

2. **`release_lock()`** — removes lock file, wired via `trap ... EXIT`

3. **`DEPLOY_ROLE` env var** — callers should set this so the lock identifies who holds it:
   ```bash
   DEPLOY_ROLE=kade ./app-state.sh deploy
   ```
   Defaults to "unknown" if not set.

## Testing Done

- **Cross-PID blocking**: Live PID lock correctly blocks second deploy ✓
- **Stale lock recovery**: Dead PID detected, lock removed, new deploy proceeds ✓
- **Re-entrant**: Same PID can call acquire_lock twice (restart→deploy path) ✓

## What You Need To Do

1. **Review the diff**: `git diff app-state.sh`
2. **Commit if it looks right**: This is your scope — I wrote it but you own the file
3. **Consider**: Should we add `DEPLOY_ROLE` to CLAUDE.md instructions for all roles? Currently each role would need to set it manually.
4. **Consider**: `system-state.sh` has the same concurrent risk for observability stack deploys — same pattern could apply there.

## Why P1

Terraform state corruption from concurrent applies is a hard-to-recover failure. This is operational safety, not a feature.

---

— Silas
