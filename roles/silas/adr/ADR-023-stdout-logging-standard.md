# ADR-023: Stdout-Only Logging, Machine-Wide

**Date**: 2026-04-15
**Status**: Accepted
**Decider**: Jeff Bridwell
**Context card**: #2008

## Context

Observability blind spot sweep (#2008) found systematic gaps: WordPress debug log in `/tmp/` invisible to Loki, `harvest-exporter` with no stdout path, and logs scattered across `/tmp/`, `~/.chorus/`, and `~/Library/Logs/`. The `/tmp/` symlink to `/private/tmp/` on macOS further complicates discovery. Deep-health check 16 catches `/tmp/*.log` drift but the find behavior is unreliable across the symlink boundary.

Services that write to files directly (not via LaunchAgent stdout) create shadow logs — invisible to Promtail, invisible to Loki, invisible to the team.

## Decision

All services log to stdout. LaunchAgents route stdout to `~/Library/Logs/{Chorus,Gathering}/`. Promtail watches those directories. No service writes to `/tmp/` or to arbitrary file paths.

**Rules:**
1. LaunchAgent `StandardOutPath` and `StandardErrorPath` must point to `~/Library/Logs/Chorus/` or `~/Library/Logs/Gathering/`
2. No log paths in `/tmp/` — the reaper (#2057) treats these as violations
3. No log paths in `~/.chorus/` for new services — existing `embed-worker` and `session-watcher` are grandfathered (Promtail covers `~/.chorus/*.log`)
4. Applications that have their own log path config (e.g., WordPress `WP_DEBUG_LOG`) must point to `~/Library/Logs/Gathering/`
5. Deep-health check 16 enforces: any non-zero `*.log` in `/tmp/` triggers a warning

## Consequences

- Every service log is visible in Loki within seconds of being written
- No shadow logs — if it's not in Loki, it's not running
- Deep-health catches drift automatically
- Two grandfathered paths in `~/.chorus/` remain until those services are next touched
