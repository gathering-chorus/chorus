# Brief: Fuseki TDB2 Verification — Confirmed, No Action Needed

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Priority**: Informational — good news, no work required

## Summary

I verified the Fuseki storage configuration today. It's TDB2 persistent storage. No migration needed.

## Findings

| Aspect | Value |
|--------|-------|
| Storage type | TDB2 (persistent triplestore) |
| Dataset name | `pods` |
| Docker volume | `{project_name}-fuseki-data` → `/fuseki` |
| JVM heap | 1GB |
| Capacity at 1GB | 4-7M triples comfortably |
| Target | 15-25M triples (across all sources, long term) |
| Scaling trigger | Query latency >500ms |
| Scaling path | Increase heap → add Lucene text index → scale out |

## How It's Configured

- `scripts/fuseki-init.sh` creates the dataset with `dbType=tdb2` via the Fuseki admin API
- Docker volume persists data between container restarts
- No assembler config files — all via API + environment variables
- `ARCHITECTURE_DECISIONS.md` in the main project already documents TDB2 as the storage layer

## Relevant to Your Backup Work

The existing `scripts/backup-pods.sh` already backs up Fuseki TDB2 data via `docker cp` from the `/fuseki` directory. When you're auditing the backup script, the Fuseki portion is snapshotting the TDB2 files — which is the correct approach for a persistent triplestore. No special export needed.

## No Action Items

This was a "know the ceiling" investigation. The ceiling is solid for current scale and has a clear scaling path. When external harvesters start adding millions of triples (photos, music), we'll bump the heap. That's a config change in `fuseki.tf`, not an architecture change.

— Silas
