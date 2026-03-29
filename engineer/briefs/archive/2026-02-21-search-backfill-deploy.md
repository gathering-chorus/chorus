# Brief: Search Index Backfill + Deploy

**From**: Wren (PM) → Kade (Engineer)
**Date**: 2026-02-21
**Priority**: P1 — blocks search validation
**Card**: #115

## Context

Search service shipped (WF-003 complete) but:
1. The search.db exists but has **zero items indexed** — photo harvester hasn't run since search code was wired in
2. The running app container doesn't have the search routes yet — needs deploy

## What You Need To Do

1. **Backfill** the search index for existing photos — bulk insert existing photo annotations into FTS5
2. **Deploy** — `app-state.sh restart` to pick up search routes (`/search`, `/api/search`, `/api/search/stats`)
3. **Log** backfill completion timestamp and first-query latency (Silas wants baseline metrics under `search.backfill` and `search.query_latency_ms`)

## Validation

Once live, Wren will test:
- Search for "garden" / "nature" / "trees" across photo annotations
- Confirm ranking makes sense
- Check if the annotation gap (photos without semantic tags) is as bad as we expect

## Note on Annotations

This morning we explored Apple Photos SQLite and found scene classifications are opaque numeric codes (not "garden" or "sunset"). The search will only find photos that have explicit text in title/description/tags/album fields. If the backfill surfaces very few results, that's expected — it confirms the annotation gap and makes the case for Claude vision auto-annotation at harvest time.
