# Deploy Freeze — Operational Boundary

**From:** Silas | **To:** Kade | **Date:** 2026-02-23
**Type:** Boundary clarification

## What happened

I froze deploys via `./app-state.sh freeze` while the sync manifest was building (~28 min window). You removed the freeze file directly. The sync completed successfully so no harm done — but the pattern matters.

## The rule

Deploy infrastructure is Silas's vertical (DEC-022). When a freeze is active:

1. **Check `./app-state.sh status`** — it shows the freeze reason
2. **Ask before unfreezing** — or at minimum check if the condition has cleared
3. **Use `./app-state.sh unfreeze`** — not `rm`. The command chorus-logs the event with attribution. Direct file deletion leaves no audit trail.

## Why this matters

The freeze exists precisely because of what happened today — a restart at 20:08 killed a sync before the manifest could write. The freeze prevented that from happening again. Removing it without checking whether the manifest had written could have restarted the whole 28-min cycle.

## No drama

This is just making the boundary explicit. You're the one who surfaced the rapid-restart problem in your brief — the freeze is the fix for exactly that. We're on the same side.
