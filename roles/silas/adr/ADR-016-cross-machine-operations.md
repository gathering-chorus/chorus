# ADR-016: Cross-Machine Operations Protocol

**Status**: Accepted
**Date**: 2026-02-25
**Context**: Three AI roles now operate across two machines (Library, Bedroom) via SSH. Without guardrails, conflicting mutations and untracked changes are inevitable. Kade already ran `kill -9` on a shared Bedroom service during debugging — exactly the pattern this prevents.

## Decision

### Read is free, write is gated.

**Any role, any machine — read operations are always safe:**
- Health checks (`curl`, `pgrep`, `lsof`)
- Log reads (`tail`, `cat` on log files)
- Status queries (`launchctl list`, `docker ps`, `df`)
- File reads for debugging

**Write/mutate operations require:**
1. A card (no card = no mutation)
2. Use managed tooling — `launchctl kickstart/kill`, `app-state.sh`, never raw `kill`/`pkill`
3. Log the action in `activity.md` with machine name

### Service registry changes go through Silas.
LaunchAgent plists (create, modify, remove) on either machine are infrastructure. Silas owns infra (DEC-022). Other roles request via brief.

### No raw process killing via SSH.
Same rule as local: no `kill`, `pkill`, `kill -9`. Use `launchctl kickstart -k` for LaunchAgent services. If that doesn't work, that's a bug to fix — not a reason to bypass.

### Machine names in all cross-machine commands.
Use "Library" and "Bedroom" (DEC-054) in activity.md entries, comments, and briefs. IP addresses in scripts only.

## Consequences

- Roles can freely monitor both machines — no permission bottleneck for reads
- Mutations are traceable (card + activity.md)
- Service lifecycle stays managed (no orphaned processes)
- Silas maintains single view of what's running where
