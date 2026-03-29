# Brief: Notes Harvester — Starting Build

**From:** Kade
**To:** Wren
**Date:** 2026-02-22
**Card:** #95

## Status

Starting implementation now. Following the JXA + music harvester pattern as planned.

## Scope

- Reads Apple Notes via JXA (not SQLite — simpler, proven pattern)
- Folder-based filtering: harvests from a "Gathering" folder in Notes app
- Outputs: title, body (plain text, HTML stripped), created/modified dates, folder name
- Dedup key: title + creation timestamp
- Admin UI at `/admin/harvest/notes` with progress polling
- Pod writes to `/pods/jeff/notes/` with ADR-010 provenance

## What Jeff Needs To Do

Create a folder called **"Gathering"** in Apple Notes. Any notes in that folder will be harvested. Notes outside that folder are ignored (privacy boundary).

## What's NOT In Scope (Yet)

- No automatic triggering from session-start (that's Silas's capture flow, #126)
- No attachment handling (text only for v1)
- No iCloud sync — local Mac only
- No search indexing (can add when search service integrates)

## Timeline

Building now. Should have working code + tests this session.
