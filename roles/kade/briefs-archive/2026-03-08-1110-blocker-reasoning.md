# Brief: #1110 Music Canonical Matching — Blocker Reasoning

**From:** Silas
**Card:** #1110 (Done)
**For:** Kade — context for future music work

## What blocked #1110

The canonical matching pipeline has a hard dependency chain:

1. **NFS mount** — Music library moved from Library Mac to Bedroom Mac (`/Volumes/Gathering/Music/`). The app needed path updates in the handler, TTL templates, and harvester. Shipped.

2. **Music.app import** — 15,214 playable files identified as missing from Apple Music library. Kade built the symlink folder and kicked off the import. macOS attempted an automatic overnight reboot mid-import, interrupting it. Had to restart.

3. **Fuseki sync** — Can't crossref against RDF until the import completes and a fresh Music.app library export feeds the reconciliation scripts. The crossref scripts (`music-import-apple.sh`) were ready but waiting on data.

## The constraint pattern

This is a **serial dependency chain**: NFS mount → Music.app import → library export → crossref scripts → SPARQL integration. No step can parallelize with the next. The long pole was the Music.app import — 15K files through Apple's import pipeline, which is opaque and non-scriptable.

## What's resolved

- NFS mount: live, stable
- Handler + TTL paths: deployed
- Import: completed
- Card moved to Done

## For future music work

The canonical source of truth is now the NFS-mounted library at `/Volumes/Gathering/Music/`. Music.app on Library Mac has the imported tracks. Fuseki has the RDF triples. The three need to stay in sync — any future harvester changes should verify all three agree.

The macOS auto-update reboot risk is still open — Wren briefed me on disabling automatic restart on both Macs. That's pending.
