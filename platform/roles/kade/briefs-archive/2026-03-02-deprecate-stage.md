# Brief: Add Deprecate stage to harvest pipeline

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #436

## Context

Jeff identified a missing final stage. After Load, the source data in ToDo and the original source location need cleanup. The pipeline should be:

**Sources → Stage → Extract → Transform → Reconcile → Load → Fuseki → Deprecate**

## What Deprecate does

For each source, after data is verified in Fuseki:

1. **Verify** — load counts match expected (safety gate, prevents premature cleanup)
2. **Archive or delete** — move `/Volumes/VideosNew/Gathering/Music/ToDo/<source>` to a Done folder, or delete if all data is verified in Fuseki
3. **Mark deprecated** — set `deprecated: true` in manifest source entry with timestamp
4. **Reclaim** — log disk space recovered

## Manifest schema addition

Add `deprecate` to the stages list for each source:
```json
"deprecate": {
  "status": "not_started",
  "action": "archive|delete",
  "verified_count": null,
  "disk_reclaimed_gb": null,
  "last_run": null
}
```

## View changes

- Add Deprecate to the pipeline legend (after Load, before Fuseki target)
- Add Deprecate stage box to each per-source card
- Green when complete, gray when not started, amber when load is done but deprecate hasn't run

## Rules

- **Never deprecate before load is verified.** The verify count must match the load count.
- **Archive by default, delete only with explicit flag.** Safer to move to Done than delete.
- This is the last step before a source is considered fully harvested.

## Also: Harden Reconcile

Jeff flagged reconcile as wobbly — more scripty/ad-hoc than repeatable. Every other stage has a clean script-to-outcome path. Reconcile should too.

Build `scripts/harvest-reconcile.sh <domain> <source-id>`:
- **Input:** source path + canonical path (from manifest `staged` fields)
- **Diff:** compare source tracks against canonical by artist/album/filename
- **Report:** overlap count, unique count, files missing, estimated size
- **Import:** rsync unique tracks into canonical (idempotent, resumable — can stop and restart)
- **Manifest update:** writes `reconcile.imported`, `reconcile.unique_tracks`, `reconcile.overlap_pct` back to manifest JSON after each batch

Same pattern as extract/transform/load — run it, it updates the manifest, the page reflects reality. Not a manual rsync with eyeball counts.

## Priority

Reconcile hardening is higher priority than Deprecate — Source #3 is mid-reconcile right now at 18%. Making it a proper script means the remaining 82% runs reliably.

Deprecate: add to view and manifest schema now so the pipeline shape is complete. Actual cleanup logic can wait until sources start finishing.
