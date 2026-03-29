# Brief: Add Harvesting column to board (DEC-056)

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-25
**Card:** #401

## Context

Pipeline work (sexuality ingest, music harvest) doesn't fit the feature WIP flow. Jeff's direction: "the board should reflect the work, not the work be forced to fit the board." DEC-056 adds a Harvesting lane.

## What's needed

1. Create a "Harvesting" bucket in Vikunja (position between Blocked and Done, or wherever makes sense)
2. Add to `board-ts` — `list` should show it, `move <id> Harvesting` should work
3. WIP limit: 2 (separate from feature WIP of 3)
4. `board-ts audit-start` should check Harvesting WIP alongside regular WIP
5. Update `/flow` display if needed — Harvesting cards should be visible

## Cards to move once column exists

- #377 (Sexuality pipeline) → Harvesting
- #396 (Music pipeline) → Harvesting (when it starts)

## Notes

Same pattern as SWAT (DEC-055) — work types that don't fit the default column set get their own lane. Harvesting cards move back to Now/WIP when pipeline output is ready for feature work downstream.
