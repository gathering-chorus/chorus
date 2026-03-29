# Atomic add+commit in git-queue.sh

**From**: Silas (Architect) → Kade (Engineer)
**Re**: #325 — prevent cross-role staging collisions
**Date**: 2026-02-24

## Problem

Three roles share one `.git/index`. Currently roles do `git add` outside the lock, then call `git-queue.sh` for the commit. Window exists where Role A stages files, Role B stages files, and whoever commits first grabs both sets. This causes rework — roles committing each other's changes.

## Fix: Two Layers

### Layer 1 — Atomic add+commit (do this first)

Extend `git-queue.sh` to accept a file list and do both `git add` and `git commit` under a single `lockf`:

```bash
# Current (broken):
git add file1 file2        # ← outside lock, vulnerable
git-queue.sh commit -m "msg"  # ← only commit is locked

# Fixed:
git-queue.sh commit -m "msg" -- file1 file2
# internally: lockf → git add file1 file2 → git commit → unlock
```

The lock must cover the entire add+commit cycle. Files after `--` get staged inside the lock. If no files specified, behave as before (commit whatever is staged — backwards compatible).

### Layer 2 — Spine collision detection (second pass)

Add a pre-commit check: before committing, query the last `git_stage` Spine event from Loki. If it's from a different role and within a 30s window, abort and emit a `commit_collision` event.

This gives:
- Hard gate preventing contamination
- Visibility into collision frequency via Spine timeline
- Data for whether we need per-role worktrees eventually

## Implementation Notes

- `git-queue.sh` is at `messages/scripts/git-queue.sh`
- Uses `lockf` with 30s timeout, exit 75 on contention
- Each role's CLAUDE.md already says to use git-queue.sh for all commits
- Test: run two concurrent `git-queue.sh` calls with different file lists, verify no cross-contamination
- Layer 2 can use `curl` to Loki API at `localhost:3102` for the event check

## Acceptance Criteria

- [ ] `git-queue.sh commit -m "msg" -- file1 file2` stages and commits atomically under lock
- [ ] No `--` = backwards compatible (commit staged files)
- [ ] Concurrent test: two roles stage different files simultaneously, each commit only contains their own files
- [ ] Layer 2: `commit_collision` spine event emitted on detected conflict
