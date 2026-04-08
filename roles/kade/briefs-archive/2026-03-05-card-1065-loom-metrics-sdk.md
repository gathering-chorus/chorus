# 1065: Loom metrics via board SDK

**From:** Wren
**Date:** 2026-03-05
**Card:** #1065 (Now, P1)
**Context:** Follows #1064 (board SDK rewrite you just shipped)

## What

Replace `loom-metrics.sh` with a Node script that imports the board SDK you just built. Add a `/api/loom/roles` endpoint for ownership tiles on the Loom page.

## Why this is next

Jeff wants to restructure /loom with role ownership at the top. Before we touch the page, the data pipeline needs to use the SDK instead of text-parsing board-ts output. You just made the SDK importable — now we use it.

## Key files

- **Kill:** `messages/scripts/loom-metrics.sh` (410 lines, bash+python, text-parses board-ts)
- **Replace with:** `messages/scripts/loom-metrics.js` (or .ts) importing `BoardClient` from board-client
- **Update:** `src/handlers/team.handler.ts` — change `execSync(bash loom-metrics.sh)` to call Node script
- **Add:** `/api/loom/roles` endpoint — per-role active cards for page header

## Current pipeline

1. `/api/loom-metrics` hits `team.handler.ts`
2. Handler runs `execSync('bash loom-metrics.sh')` with 15s timeout
3. Script hits Vikunja API via curl, text-parses `board-ts list` and `board-ts buckets`, reads chorus.log with inline Python
4. Returns JSON to page

## New pipeline

1. `/api/loom-metrics` hits `team.handler.ts`
2. Handler imports and calls a compute function from the new metrics module
3. Module uses `BoardClient.list()`, `BoardClient.listGrouped()` directly
4. Spine event data still from chorus.log (that's fine — it's local and fast)

## AC

- Same JSON shape from /api/loom-metrics (backward compatible with team.ejs)
- No text-parsing of CLI output
- New /api/loom/roles endpoint returns active cards per role
- Page loads under 2s
