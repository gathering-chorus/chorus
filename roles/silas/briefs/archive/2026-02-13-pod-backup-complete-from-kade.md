# Brief: Pod Data Backup — Implementation Complete

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-02-13
**Re:** `2026-02-13-pod-backup-scope-for-kade.md`

## Status: Phases 1–2 Shipped, Cron Active

Picked up your brief, read the existing skeleton, rewrote it from scratch. All on `main` now.

## Against Your Definition of Done

- [x] **Automated backup runs on schedule** — Cron installed, daily at 2 AM local
- [x] **Backup includes pods, ontology, and Fuseki data** — All three in every archive (170 pod files + 2 ontology + 56 Fuseki TDB2 files). Fuseki pulled live via `docker cp` from the running container.
- [x] **Rotation prevents unbounded disk growth** — 7 daily + 4 weekly (28 days). macOS-compatible `find -mtime`, no GNU-isms.
- [x] **Restore verification runs after each backup and logs result** — `verify-backup.sh` extracts to temp, validates file counts, checks all 5 `.meta.ttl` files by name, verifies both ontology files, checks Fuseki data if included. Auto-runs at end of backup.
- [x] **Backup success/failure visible in Grafana via Loki logs** — All output is structured JSON (`{"app":"pod-backup",...}`). Verification failures log at `error` level for alerting.

## What I Found in the Skeleton

The existing `scripts/backup-pods.sh` had several issues:
- Sourced `.env.pod` via a fragile relative path (`./../../../../.env.pod`) — broken from any working directory
- Used `find -printf` — GNU-only, broken on macOS
- Only backed up pods (no ontology, no Fuseki)
- No restore verification
- No cron scheduling
- No rotation by age — used `head -n -7` which is also GNU-only

Rewrote entirely. Same file path, clean replacement.

## Scripts Delivered

| Script | Purpose |
|--------|---------|
| `scripts/backup-pods.sh` | Full backup with rotation and verification |
| `scripts/verify-backup.sh` | Restore verification (file counts, key files) |
| `scripts/install-backup-cron.sh` | Idempotent cron installer |

## Design Decisions

- **Fuseki backup is best-effort.** If the container isn't running, logs a warning and continues — pods + ontology still get backed up. Didn't want a missing container to block the critical data from being protected.
- **Staging via temp dir.** Copy all three data sets to a temp dir, then single `tar -czf`. Cleaner than chaining multiple tar sources and ensures atomic archiving.
- **Weekly rotation on Sundays.** Copies the day's daily to the weekly dir. Weeklies expire at 28 days.

## Open: Phase 3 (Off-Machine Copy)

Not started. Ready to pick this up when you and Jeff want to scope it — need to know if there's an S3 bucket, rsync target, or other destination preference.

## Archive Stats (First Run)

- Size: ~28MB compressed
- Contents: 269 entries (170 pod + 2 ontology + 56 Fuseki + directories)
- Verification: PASSED

— Kade
