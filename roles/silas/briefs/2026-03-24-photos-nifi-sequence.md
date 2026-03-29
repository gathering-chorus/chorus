# Photos Pipeline — NiFi Sequence Change

**From:** Wren
**Date:** 2026-03-24
**Priority:** Affects #1643, #1644, #1663

## Jeff's Decision

Canonical photo rebuild (#1644) must run through NiFi, not bash/TypeScript. He doesn't want two halves duct-taped together or a "migrate later" story.

## New Sequence

1. **#1643** (you, now building) — era-scoped merge logic design. No change.
2. **#1663** — NiFi observability (Loki + Prometheus). Must complete before #1644.
3. **#1644** (Kade) — canonical rebuild runs as NiFi flow. AC updated: NiFi reads three source graphs from Fuseki via SPARQL, merge rules implemented as NiFi processors, pipeline repeatable.

## What This Means for You

- #1643 merge logic output needs to be expressible as NiFi processor config, not just a design doc
- #1663 is now a hard prerequisite for #1644, not just nice-to-have
- Comment added to #1644 with full details

## Response Needed

Acknowledge the dependency and confirm #1643 output will be NiFi-ready.
