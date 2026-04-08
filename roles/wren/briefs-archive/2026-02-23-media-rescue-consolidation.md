# Brief: Media Rescue & Consolidation

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-23
**Context:** Disk at 93% on primary Mac. Jeff directed investigation → discovered ~1.5TB of media across Downloads/iTunes (988GB), Music (484GB), Pictures. Jeff wants to move media to secondary Mac, but needs harvest pipeline to maintain links back to playable/viewable content first.

## The Opportunity

Jeff has a messy but rich media estate: iTunes library backups, Apple Music, Photos library, scattered across directories from a lost hard drive recovery with multiple backup sources. He wants to:

1. **Free disk on primary Mac** — move ~1TB+ of media to secondary Mac (200TB external storage)
2. **Harvest everything** — catalog all content into the RDF graph with full provenance
3. **Maintain playback/view links** — the graph should trace back to the actual MP3/MP4/JPEG, not just metadata
4. **Deduplicate and validate** — same content may exist in multiple locations from backup recovery

## Current State (Gap Analysis)

| Harvester | Source | Stores file path? | Can trace to media? |
|-----------|--------|-------------------|-------------------|
| Music | JXA → Apple Music app | No | No — no file path or persistent ID in RDF |
| Photos | SQLite → Photos.sqlite | Partial — UUID + filename, no full path | Thumbnails only, can't open original |

**Neither harvester currently stores the source file path as a triple.** The graph is metadata-only — it can't point you back to the playable file.

## What's in the Trees

- `~/Downloads/iTunes/` — old iTunes library backup (pre-Apple Music migration?)
- `~/Downloads/Pictures/` — photo library or backup, includes `Photos Library.photoslibrary` (46.8GB)
- `~/Downloads/Music/` — another music folder
- `~/Music/` — current Apple Music library (484GB)
- `~/Pictures/` — current Photos library

Overlap is likely. Some content may exist in 2-3 places. Some may be orphaned (metadata in Apple's DBs but files moved/lost).

## Proposed Architecture Changes

### 1. Add `jb:sourceFilePath` to harvest output
Both harvesters write a triple linking each resource to its source file on disk. This is the provenance chain.

### 2. Add `jb:sourceLocation` for migration tracking
When files move (primary → secondary Mac), update the location triple without re-harvesting metadata. The graph knows where the file lives now.

### 3. Inventory-first harvest mode
Before full harvest, run a lightweight scan that catalogs:
- What files exist in each tree (type, size, modification date)
- What's already been harvested (match against existing RDF)
- What's duplicated across trees
- What's orphaned (in DB but file missing, or file exists but not in DB)

### 4. Sequenced migration
1. Inventory all three trees (iTunes, Pictures, Music)
2. Harvest with file paths into RDF
3. Validate: every RDF resource has a reachable source file
4. Move media to secondary Mac (rsync with verification)
5. Update `jb:sourceLocation` triples to new paths
6. Verify: every RDF resource still resolves
7. Delete from primary Mac only after verification passes

## Why This Matters

- **Disk freedom**: Gets primary Mac from 93% → ~40% (recovering ~1TB)
- **Content rescue**: Jeff's digital life scattered across backup recovery — this brings it together
- **Harvest pipeline maturity**: Forces the pipeline to handle real-world messiness (dupes, missing files, multiple sources)
- **Data provenance**: The graph becomes a true index into Jeff's media, not just a metadata mirror
- **Cultivating domain validation**: This is the kind of complex, stateful, multi-source harvesting that the Cultivating UX needs to handle

## Sizing

This is a multi-session project. Rough phases:
- **Inventory spike** (1-2 sessions): scan trees, estimate overlap, size the problem
- **Harvester updates** (2-3 sessions): add file path triples, inventory mode, migration tracking
- **Harvest + validate** (2-3 sessions): run against all three trees, deduplicate, verify
- **Migration + verify** (1-2 sessions): rsync to secondary, update triples, confirm

## Recommendation

Card this as a P1 epic. It solves the disk pressure (operational), rescues content (personal value), and matures the harvest pipeline (product). Jeff signaled this could be "the next big thing we do."

— Silas
