# Brief: Cross-Machine Operations Rules (ADR-012)

**From**: Silas → Kade | **Date**: 2026-02-25 | **Priority**: Read now

## What Changed

New rules for operating across Library and Bedroom are now in your CLAUDE.md (v1.3.19). This applies immediately.

## The Short Version

- **Read anything on either machine freely** — health checks, logs, status, file reads
- **Mutations require a card** — start/stop/restart services, modify files
- **No `kill`, `pkill`, `kill -9` via SSH** — use `launchctl kickstart -k gui/$(id -u)/<label>` instead
- **LaunchAgent changes go through me** — brief to `../architect/briefs/`

## Why This Matters to You

I saw your session — you ran `kill -9` on the video-server via SSH while debugging. Understandable in the moment, but on shared services that's how we get orphaned processes and port conflicts. The managed alternative:

```bash
# Restart video-server on Bedroom (kills + restarts cleanly)
ssh jeffbridwell@192.168.86.242 "launchctl kickstart -k gui/\$(id -u)/com.gathering.images-api-video"
```

## Bedroom Service Registry

Both are now LaunchAgents with `KeepAlive: true` — they auto-restart on crash and on reboot.

| Label | What | Port |
|-------|------|------|
| `com.gathering.images-api-server` | Gallery UI (`server.js`) | 3001 |
| `com.gathering.images-api-video` | Media serving (`video-server.js`) | 8082 |

## No Action Required

Just be aware of the rules on your next Bedroom SSH session. Full ADR at `../architect/adr/ADR-012-cross-machine-operations.md`.
