# Media Rescue — Technical Inventory

**From**: Wren (PM) → Kade (Engineer)
**Re**: #311 — Music library rescue (expanding to full media)
**Date**: 2026-02-24

## What I Need

A technical inventory of Jeff's media landscape. You built the music and photos harvesters — you know what's indexed and what's dark. This is discovery work, not building yet.

## Scope

Jeff's Downloads folder has iTunes + Pictures folders (~1.5TB) used as harvest targets. There's a lost hard drive, multiple backups, and 6+ partial iTunes/Music library copies across both Macs. Neither the music nor photos harvester stores source file paths in RDF — that's the architectural gap that makes rescue hard.

## Deliverables

1. **Inventory**: What media directories exist on both Macs? Sizes, formats, rough file counts. Include the SMB mount from secondary Mac (~7GB Gathering mount, but ~200TB external storage total).

2. **Harvester coverage map**: What do the current music/photos harvesters actually index? What fields? What's missing (notably: source file path, dedup identifiers)?

3. **Duplication estimate**: Any quick signal on how much overlap exists between the known library copies? Even rough is fine — "these two dirs share ~60% of filenames" level.

4. **Gap analysis**: What would it take to add source-path tracking to the harvesters? Is that a small change or an architectural one?

## Constraints

- **Read-only**. Don't move, rename, or delete anything. This is inventory only.
- **Don't harvest**. Don't run harvesters or ingest anything new.
- **Search BOTH Macs.** Primary (192.168.86.36, local) and secondary (192.168.86.33, 3rd floor). The secondary has ~200TB external storage — that's where a lot of media lives. SMB mount may or may not be active. If it's not mounted, try to mount it or note what you can't see.
- Time-box this to one session. Broad strokes, not exhaustive counts.

## Why This Matters

Jeff said "maybe this is the next big thing we do." The media is his personal collection — music, photos, memories. It's scattered and partially dark. Before we can plan a rescue, we need to know what we're rescuing.

## Role Split

- **You (Kade)**: This inventory — technical analysis of what exists and what the harvesters cover.
- **Silas**: Storage topology (he already has infra context from the Fuseki audit and disk work).
- **Wren**: Product framing — what "rescued" means, priority order, how it connects to Self domain in Gathering.

Ship your findings back as a brief to `product-manager/briefs/`.
