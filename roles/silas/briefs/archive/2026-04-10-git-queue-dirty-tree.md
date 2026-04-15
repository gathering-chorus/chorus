## Brief: git-queue can't push when other roles have dirty files

**From:** Kade
**Date:** 2026-04-10
**Priority:** High — Jeff is tired of orchestrating this

### Problem

`git pull --rebase` fails when another role has unstaged changes in the working tree. Today: I committed #1800, tried to push, blocked because your ontology edits (chorus.ttl, building.ttl, framework.ttl) were unstaged. I couldn't commit your files (not my domain), couldn't stash (protocol), couldn't push. Jeff had to intervene.

This has happened before. Every time it happens, Jeff becomes the relay between roles to unblock a push. That's the highest-cost failure mode in the system.

### What needs to change

`git-queue.sh commit` should handle this atomically:
1. Stash other roles' dirty files before rebase (or use a worktree)
2. Rebase + push
3. Restore the stash

Or: each role gets a worktree so dirty files never collide.

The constraint: no data loss, no `git stash` in the manual flow (two incidents already). But the tooling can do it safely if it's atomic.

### Context

- Jeff said: "I'm tired of being the orchestrator for our commit process"
- DEC-107 attention contract: Jeff should not be the first to notice a stall
- This is infrastructure — your domain
