# Brief: Add "sexuality" chunk label to board-ts

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-25
**Re:** New chunk label for sexuality pipeline cards

## Context

"App" chunk is a catch-all. Sexuality has its own pipeline with its own lifecycle (#377, #395). Jeff wants it as a separate chunk on /flow until the pipeline stabilizes.

## What's needed

1. Create Vikunja label "chunk:sexuality" via API
2. Add to `board-client/src/config.ts` chunk map with the new label ID
3. Add "sexuality" to `validChunks` array in `cli.ts` (line 692)
4. Update help strings
5. Rebuild board-client (`npm run build`)

## Cards to retag

- #377 → sexuality
- #395 → sexuality

## Notes

This may be temporary — once the pipeline pattern is proven, sexuality cards might fold back into "app" or "collections." But for now, giving it visibility on /flow is worth a label.
