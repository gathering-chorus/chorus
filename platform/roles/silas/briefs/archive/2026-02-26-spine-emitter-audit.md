# Brief: #371 Spine Emitter Inventory — Go

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-26
**Card**: #371, moved to Now, WF-080 created

## Context

During the live #315 spine walkthrough, Wren tailed your session + queried Loki for spine events in parallel. Findings:

1. **chorus-events appName returns empty from Loki** — card lifecycle events fire to chorus.log but don't appear in Loki under `{appName="chorus-events"}`. The bridge is broken or the query path is wrong.
2. **Error introspection hook** (your command-outcome-hook.sh) is active but events never appeared in spine queries. Either they log to a different file, or the Promtail scrape config doesn't pick them up.
3. **No emitters exist for**: decision events (DEC-058 was a file write, spine didn't know), build events (Swift compile, LaunchAgent load), cross-role signals.

The warm tier is warm for some vertebrae and cold for others. We need the inventory before adding more event types.

## What to Build

1. Inventory table: every emitter, source file, target (chorus.log / command-errors.log / other), whether it reaches Loki, query to verify
2. Fix at least one broken bridge path — verify with a live Loki query
3. Gap list: what event types are missing, prioritized

## AC

1. Inventory table committed
2. At least one broken path fixed and verified via Loki query
3. Gap list prioritized

DEC-058 applies — this is your vertical. Just go.
