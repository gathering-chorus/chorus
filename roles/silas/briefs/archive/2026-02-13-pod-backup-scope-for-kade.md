# Brief: Pod Data Backup — Build Scope

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Priority**: High — highest-risk non-functional gap in the system
**Context**: ADR-003 is shipped. Nice work on all 8 steps. This is next.

## Why This Matters

The pods directory is the single point of failure for the entire knowledge graph. Every Turtle file, every `.meta.ttl` visibility declaration you just built, every resource Jeff has authored or curated — one filesystem, no copy. Disk failure = permanent loss of the semantic memory layer.

You just made `.meta.ttl` files load-bearing for access control. The stakes on this directory went up today.

## What to Protect (priority order)

1. **Pod Turtle files** — the knowledge graph. Irreplaceable.
2. **`.meta.ttl` files** — visibility declarations, now load-bearing (ADR-003).
3. **Ontology files** (`src/ontology/`) — versioned in git but runtime copies matter.
4. **Fuseki TDB2 data** — reconstructable from pods via re-index, but slow at scale. Worth snapshotting to avoid a full rebuild.

## Existing Work — Check First

During the Fuseki TDB2 verification I found `scripts/backup-pods.sh` already exists. It backs up both pods and Fuseki TDB2 data with 7 daily + 4 weekly rotation. **Read this script first** — some of Phase 1 may already be done. Your job may be to verify it, fill gaps (restore verification, off-machine copy, observability), and make sure it's actually running on a schedule.

## Phase 1 — Minimum Viable Backup

- Cron job (shell script) that tars the pods directory to a second local volume
- Schedule: daily minimum. Hourly if disk I/O cost is negligible at current pod size.
- Rotation: keep 7 daily, 4 weekly. Delete older.
- Include the ontology directory in the backup set.
- Include the Fuseki TDB2 data directory in the backup set.
- Log success/failure to stdout so Promtail picks it up and it appears in Grafana.

This is a shell script and a cron entry. Keep it simple.

## Phase 2 — Verify It Works

- Restore test script: untar to a temp directory, count files, compare file count against source.
- Run automatically after each backup completes.
- Log the result — a backup that hasn't been tested isn't a backup.
- If verification fails, log at error level so Grafana alerts are possible.

## Phase 3 — Off-Machine Copy (stretch)

- Push the latest tar to a second location: rsync to another machine, or S3 if Jeff has a bucket.
- Local backup protects against accidental deletion. Off-machine protects against hardware failure.
- This is the real protection. Phases 1-2 are the minimum; Phase 3 is the goal.

## What to Skip

- **Incremental/differential backups** — at current pod size, full tar is fine. Don't over-engineer.
- **Fuseki-specific backup tooling** — snapshot the TDB2 directory in the same tar job. No special handling needed.
- **Point-in-time recovery** — that's database thinking. Not needed at this scale.
- **Compression optimization** — gzip the tar, done. Don't tune it.

## How This Fits

- This runs in parallel with my Fuseki TDB2 verification work. No dependency between them.
- The backup script lives in the infrastructure layer — could be in `jeff-bridwell-personal-site` or in a small ops directory. Your call on where it fits cleanest.
- Wren has this on the priority stack for the kanban board (item #2, right after your ADR-003 work).

## Definition of Done

- [ ] Automated backup runs on schedule without manual intervention
- [ ] Backup includes pods, ontology, and Fuseki data
- [ ] Rotation prevents unbounded disk growth
- [ ] Restore verification runs after each backup and logs result
- [ ] Backup success/failure visible in Grafana via Loki logs

Phase 3 (off-machine copy) is a follow-up, not a blocker for "done."

Good work on ADR-003. This one is less interesting but more important.

— Silas
