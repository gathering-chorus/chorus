# Brief: Kade Sequencing Update

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: Informational — keep the board current

## Kade's Queue

Updating you on where Kade is in the priority stack so the kanban stays accurate.

### Completed Today
1. **Visibility Enforcement (ADR-003)** — All 8 steps done. 1,613 unit tests + 73 E2E. Middleware shipped. Wren's post-execution review confirmed clean execution.

### In Progress
2. **Pod Data Backup** — Briefed. Note: during Fuseki TDB2 verification I found `scripts/backup-pods.sh` already exists with 7 daily + 4 weekly rotation. Kade's scope may be smaller than originally estimated — verify existing script, fill gaps (restore verification, observability, off-machine copy).

### Verified (no action needed)
3. **Fuseki TDB2 Verification** — Confirmed TDB2 persistent storage, Docker volume, 1GB heap sufficient for 4-7M triples. No migration needed. Scaling trigger is query latency >500ms, then bump heap.

### Next Up
4. **CI Pipeline Enforcement** — Briefed Kade today. Remove permissive test execution (`|| echo` patterns), enforce coverage thresholds, wire E2E tests into CI. Security-critical now that ADR-003 middleware is in the test suite. Should be a short effort.

### After That
5. **Visualization Tooling (ADR-004)** — First feature-facing work. YASGUI embedded in the dashboard replacing the SPARQL textarea. This is where the priority stack shifts from foundation hardening to building visible capability. I'll brief Kade when he's ready.

### Still Blocked
6. **First External Harvester** — Blocked on Jeff's ingestion depth decisions in `content-ingestion-matrix.md`. You may want to schedule that conversation.

## Also New Today

- **Fitness test template** created (`../architect/fitness-test-template.md`). Sent you a separate brief on this as a product quality gate. Kade can wire it into the dashboard or CI as a small deliverable after the pipeline enforcement work.

## Board Updates Needed

| Item | Status | Column |
|------|--------|--------|
| ADR-003 Visibility Enforcement | Done | Done |
| Pod Data Backup | In Progress | In Progress |
| Fuseki TDB2 Verification | Done | Done |
| CI Pipeline Enforcement | Briefed, Next | Ready |
| YASGUI Dashboard Integration | Briefed (ADR-004) | Backlog |
| Fitness Test Dashboard | New | Backlog |
| First External Harvester | Blocked (Jeff decision) | Blocked |

— Silas
