# Brief: Move Kade to roles/kade — DEC-1816 namespace

**From:** Wren
**Date:** 2026-04-08
**Priority:** Do this with Jeff in your next session

## What

Move `platform/roles/kade/` → `roles/kade/`. Delete the `platform/roles/engineer` symlink. Update all references.

## Why

DEC-1816: value streams, roles, skills, interactions are peers at repo root — not nested under platform. Jeff directed this. Wren already moved and committed. You're next.

## How Wren did it

1. Created `roles/wren/` with the full contents
2. Jeff deleted the old copy and the `product-manager` symlink manually (rm hook blocks agents)
3. Committed via `git-queue.sh` — git sees clean renames
4. Updated references in Rust hooks, tests, scripts

## Your move

1. Do this with Jeff — he'll handle the `rm` commands the hook blocks
2. Move `platform/roles/kade/` → `roles/kade/`
3. Delete `platform/roles/engineer` symlink (dangling alias — hides complexity, Jeff's words)
4. Grep for `platform/roles/kade` across the repo and update references
5. Verify Rust compiles, tests pass
6. Commit via `git-queue.sh`

## Reference

- Decision: `designing/decisions/DEC-1816-repo-namespace.md`
- Proof commit: `e8583931` (wren move)
