# Harvester Source Path Tracking — Build Now

**From**: Wren (PM) → Kade (Engineer)
**Re**: #311 — Media rescue, pre-harvest prep
**Date**: 2026-02-24

## Direction

Jeff wants to play music through Gathering after harvest. That means source file paths must be in the RDF *before* we harvest — we don't want to re-harvest 80K tracks.

Build the source path changes now. The copy to /Gathering is still running (~24h). Code changes don't depend on it.

## What to Build

### Music Harvester
1. Add `location: t.location()` to the JXA extraction script
2. Add `location?: string` to `MusicTrackRaw` interface
3. Thread `location` through to `TrackResource` and Turtle output as `jb:sourceFilePath`
4. Handle `undefined` gracefully (Apple Music streaming tracks have no local file)

### Photos Harvester
1. Add `directory?: string` to `PhotoItemRaw` interface
2. Include `directory: row.directory` in the SQLite mapping (it's already queried, just dropped)
3. Thread through to `PhotoResource` and Turtle output as `jb:sourceFilePath`
4. Bonus: extract `ZADJUSTEDFINGERPRINT` (content hash) and `ZORIGINALFILESIZE` — free data from the same SQLite query

### Ontology
- New predicate: `jb:sourceFilePath` (string, absolute path to playable/viewable file)
- New predicate: `jb:sourceFileHash` (string, content fingerprint — photos only for now)
- Brief Silas on predicate naming if you want his input, but don't block on it

## Don't Do Yet
- Streaming endpoint — comes after harvest
- Play button UI — comes after streaming
- Docker bind-mount — Silas handles, comes at deploy time
- Don't run the harvest — wait for copy to complete

## Why Now
One harvest. Get it right. Every field we miss means re-harvesting 80K music tracks and 44K photos.
