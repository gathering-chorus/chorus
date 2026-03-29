# Media Harvest Direction — Phase 1

**From**: Wren (PM) → Kade (Engineer)
**Re**: #311 — Response to photos reharvest question + inventory findings
**Date**: 2026-02-24

## Decision

Phase 1: commit what you have, harvest what we can reach locally. Don't wait for the full convergence problem.

## What to Do Now

1. **Commit source path tracking** — both harvesters, tsc clean, 2310 tests passing. Ship it.
2. **Re-harvest Apple Photos** on primary Mac — backfill `jb:sourceFilePath`, `jb:sourceFileHash`, `jb:originalFileSize` on the ~44K photos already in RDF.
3. **Re-harvest Apple Music** on primary Mac — backfill `jb:sourceFilePath` on the ~98K tracks.
4. **Don't touch the secondary Mac, external drives, or loose files yet.** That's Phase 2.

## What's NOT in Scope

- File-scan harvester for loose photos (needs design)
- iTunes XML parser for legacy libraries
- Remote harvesting from secondary Mac
- Cross-library dedup
- images-api → Fuseki bridge (Silas territory)

## Why This Order

Jeff wants to get music loaded so he can explore traversing a data set like memory — including being able to listen to it. Source paths are the link between RDF metadata and the actual playable file. Phase 1 gets us there for everything on the primary Mac.

The 175K photos on external storage + 44K additional music files are real and important — but they need plumbing that doesn't exist yet (file-scan harvester, remote access). We'll get there. Not today.

## Context

Jeff described the full problem this morning: data scattered across two Macs, multiple volumes, no unified inventory. images-api on the second Mac has 1.85M items in MongoDB across 29 external volumes. That's the Phase 2 bridge — syncing MongoDB → Fuseki via collection graph sharding.

The sequencing: local harvest (now) → images-api bridge (next) → new harvesters for loose files and remote volumes (later).

## Acceptance

- Source path tracking committed
- Apple Photos re-harvested with all three new fields populated
- Apple Music re-harvested with source paths populated
- Verify: pick a random track/photo in Fuseki, confirm source path resolves to a real file
