# Brief: Harvest Pipeline Architecture — Music (#396)

**From**: Kade | **To**: Silas | **Date**: 2026-02-27
**Card**: #396 — Music pipeline — consolidate sources, dedupe, complete harvest

## Context

Jeff sketched the harvest pipeline architecture on paper. I rendered it as an interactive HTML diagram with live numbers from the current system state.

**Diagram**: `engineer/harvest-pipeline.html` (open in browser)

## What It Shows

Three music sources funnel through a transform (Tx) into the target RDF graph:

| Source | Location | Files |
|--------|----------|-------|
| #1 (canonical) | ~/Music/Music (Primary Mac) | 100,658 |
| #2 (backup) | Gathering/iTunes (Secondary SMB) | 98,993 |
| #3 (legacy) | PhotosNew/iTunes Music (Secondary) | 41,690 |

**Current gap**: Only 17,086 tracks in Fuseki (17% of source #1). Last harvest ran partial on Feb 24.

## Jeff's Design Questions (from the sketch)

- **Static?** — Ingest once, merge to canonical. Right answer for now.
- **Dynamic?** — Listener for incremental changes. Not needed yet.
- **Count each source — must match target.** This is the acceptance test.

## Infra Considerations for You

- Full 100K extraction via JXA runs on the host (not in Docker). May need the JSONL file volume-mounted or copied into the container.
- Spine events `harvest_start` / `harvest_complete` exist in schema but aren't wired in the harvester yet. I'll wire them via `chorus-log.sh`.
- Sources #2/#3 are on secondary Mac — SSH read is fine (read is free per ADR-012), but bulk file comparison for dedup may be slow over SMB.

## Next Steps (Kade)

1. Re-extract source #1 completely (JXA on host)
2. Wire spine events into harvester
3. Run full ingest, verify count matches
4. Dedupe sources #2/#3 against #1

— Kade
