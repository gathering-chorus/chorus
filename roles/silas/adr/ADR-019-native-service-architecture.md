# ADR-019: Native Service Architecture

**Date**: 2026-04-15
**Status**: Accepted
**Decider**: Jeff Bridwell
**Supersedes**: ADR-001, ADR-011, ADR-012, ADR-015

## Context

Docker was removed from the Gathering/Chorus stack in March 2026. All services now run as native macOS LaunchAgents managed by `agent-state.sh` and `launchctl`. This ADR documents the architecture that replaced Docker — it is the binding reference for how services run.

## Decision

### Service Lifecycle

All services run as LaunchAgents with `KeepAlive: true` and `RunAtLoad: true`. Lifecycle management:
- **Start/stop/restart:** `agent-state.sh start|stop|restart <name>`
- **Health check:** `agent-state.sh health`
- **Never kill PIDs manually.** Use `launchctl kickstart` for agents, `agent-state.sh` for Docker-era services still referenced.

### Canonical Plists

Canonical plist definitions live in `chorus/platform/scripts/launchagents-canonical/`. Deployed to `~/Library/LaunchAgents/`. Changes to plists go through Silas (ADR-016).

### Logging

Services write to stdout/stderr. LaunchAgent plists route output to:
- `~/Library/Logs/Chorus/` — Chorus services
- `~/Library/Logs/Gathering/` — Gathering services
- `/tmp/` — ephemeral logs (not shipped to Loki)

Promtail watches log directories and ships to Loki (port 3102). See DEC-101 for the stdout-only logging standard.

### Two-Machine Topology

- **Library** (192.168.86.36): Primary machine. All core services.
- **Bedroom** (192.168.86.242): Secondary. Promtail, Ollama, Images API, Navidrome, node-exporter.

Cross-machine operations follow ADR-016 (read free, write gated).

### Port Assignments

| Service | Port | Process |
|---------|------|---------|
| Gathering App | 3000 | Node.js (Express) |
| Fuseki | 3030 | Java |
| Grafana | 3100 | Native binary |
| Loki | 3102 | Native binary |
| Chorus API | 3340 | Node.js (Express) |
| Vikunja | 3456 | Native binary |
| Clearing | 3470 | Node.js (Socket.IO) |
| Messaging | 3475 | Node.js |
| Prometheus | 9090 | Native binary |
| Alertmanager | 9093 | Native binary |
| Node Exporter | 9101 | Native binary |
| Chorus Hooks | Unix socket | Rust (Axum) |

### Network Binding (Open Concern)

Services currently bind to `0.0.0.0` (all interfaces). Localhost-only binding is desirable for non-Cloudflare services but not yet enforced in native LaunchAgent configs. This is a security concern inherited from ADR-012 that was not resolved during the Docker removal.

### Deploy Patterns

- **Views/CSS**: Bind-mounted (no deploy needed)
- **TypeScript changes**: `app-state.sh deploy` (rebuild dist/ + restart)
- **Rust binary changes**: `cargo build --release` + `agent-state.sh restart`
- **Plist changes**: Copy to ~/Library/LaunchAgents/ + `launchctl bootout` + `launchctl bootstrap`

### Rollback

Git-based: `git revert HEAD` + redeploy via `app-state.sh` or `agent-state.sh restart`. No image tags, no container layers — rollback is a code operation.

## Consequences

- No Docker dependency. Docker Desktop should not be started (see feedback memory).
- All service state is local filesystem — no volume mounts, no container layers.
- LaunchAgent KeepAlive provides auto-restart on crash.
- Log aggregation depends on Promtail watching the right directories — new services must have their log paths added to `promtail-native.yaml`.
