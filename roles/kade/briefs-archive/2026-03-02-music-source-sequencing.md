# Brief: Music harvest source sequencing — one at a time

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #436

## Context

Jeff directed: finish Bedroom MP3 (Source #3) since extract is already done, then sequence the remaining sources one at a time. Yesterday three sources ran concurrently — I/O contention made everything slower. Pipeline stages are I/O-bound (XML parse, disk scan, Fuseki PUTs). Sequential is faster than concurrent here.

## Sequencing

Finish current, then work through in this order. Each source fully through extract → transform → load before starting the next.

1. **Source #3 — Bedroom MP3 Collection** — CURRENT. Extract done (41,647). Reconcile at 18% (3,855/16,934). Finish the import, then transform + load the unique tracks.
2. **Source #4 — Library iTunes (Nov 2019)** — XML available (170MB), not parsed. Extract next.
3. **Source #5 — Library Music Snapshot (Feb 2021)** — Diff against Source #1 first. If pure duplicate, mark done and skip. If unique tracks exist, extract the delta.
4. **Source #2 — Bedroom iTunes (SMB)** — Blocked on volume mount. Skip until available.
5. **Source #6 — Kirby (2010 PC backup)** — Small (2,472 items). Triage after the big ones.
6. **Source #7 — Previous Libraries** — Metadata only. Last, low priority.

## Rules

- **One source flowing at a time.** No concurrent pipeline runs.
- **Update the manifest after each stage completes** (per the auto-update brief sent earlier today).
- **Don't deploy per-stage.** Batch pushes — the cooldown guard in app-state.sh will reject deploys within 3 minutes of each other anyway.

## Goal

All sources extracted, reconciled, and loaded into Fuseki under the canonical Apple Music graph. Then the old source locations become read-only — Gather → Match → Deprecate.
