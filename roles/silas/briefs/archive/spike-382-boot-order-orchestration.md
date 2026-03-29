# Spike: Boot-Order Orchestration (#382)

**Author**: Silas | **Date**: 2026-02-25 | **Status**: Recommendation ready

## Problem

Mac reboot = manual recovery across 4 compose files + 15 containers. This morning: Docker Desktop didn't auto-start, port forwarding broke silently, cascading issues. As harvesters scale, this becomes an incident, not an inconvenience.

## Current State

- **Docker Desktop AutoStart**: `False` (not enabled)
- **Restart policies**: All 15 containers use `restart: unless-stopped` — but this only works once the daemon is up, and provides no cross-stack ordering
- **5 Chorus LaunchAgents** already running — established pattern
- **Secondary Mac**: images-api (2 bare Node.js processes) has zero auto-start — dies on reboot, requires manual SSH + `node video-server.js`

## Service Dependency Graph

```
Phase 1 — Network prerequisite
  observability-network (external, must exist)

Phase 2 — Data & infrastructure (parallel, ~30s)
  ├── loki, prometheus, alertmanager, exporters
  ├── fuseki (longest: 60s start_period)
  ├── mysql, mailhog
  ├── vikunja
  └── webvowl

Phase 3 — Dependent services (gated on Phase 2 health)
  ├── promtail (← loki healthy)
  ├── grafana (← prometheus + loki started)
  ├── app (← fuseki healthy)
  └── wordpress (← mysql healthy + mailhog started)
```

Wall time to full steady state: ~90 seconds.

## Recommendation: LaunchAgent + Startup Script

**Why not just Docker auto-start + restart policies?**
Gets you 80%. The 20% gap: no cross-stack ordering on daemon restart, and this morning's bug (port forwarding dead despite healthy container) requires a host-level health gate that Docker can't provide.

**Approach (3 pieces):**

### 1. Enable Docker Desktop AutoStart
Flip `AutoStart: true` in Docker Desktop settings. Foundation layer — gets the daemon running on login.

### 2. `docker-startup.sh` — Ordered boot script (primary Mac)

```
Wait for Docker daemon (poll `docker info`, max 120s)
  → Start observability stack (compose up -d)
  → Wait for loki + prometheus healthy
  → Start vikunja, wordpress (compose up -d, parallel)
  → Start app stack (compose up -d)
  → Wait for fuseki healthy → app healthy
  → Host-level port validation (curl localhost:3000, 3100, 3456)
  → Log summary
```

Idempotent — safe to re-run. Restart policies handle the fast path; script fixes ordering and validates host-level connectivity.

### 3. LaunchAgents

| Mac | Plist | What | KeepAlive |
|-----|-------|------|-----------|
| Primary | `com.chorus.docker-services.plist` | Runs `docker-startup.sh` on login | No (run-once) |
| Secondary | `com.gathering.images-api-server.plist` | `node server.js` (port 3001) | Yes |
| Secondary | `com.gathering.images-api-video.plist` | `node video-server.js` (port 8082) | Yes |

Must be **LaunchAgent** (not LaunchDaemon) — Docker Desktop needs the GUI session.

### 4. Wire into app-state.sh

Add `app-state.sh boot` command — calls the startup script. Keeps single control plane pattern.

## What This Fixes

| Today | After |
|-------|-------|
| Reboot → manual startup across 4 dirs | Reboot → login → all services auto-recover |
| Port forwarding breaks silently | Host-level health gate catches it |
| Secondary Mac media serving dies | LaunchAgent + KeepAlive auto-restarts |
| No boot log | `/tmp/docker-startup.log` |

## Alternatives Considered

| Approach | Verdict |
|----------|---------|
| Docker auto-start + restart policies only | 80% solution — no cross-stack ordering, no host port validation |
| pm2 for secondary Mac | Overkill for 2 processes that rarely crash |
| LaunchDaemon (pre-login) | Docker Desktop can't run without GUI session |

## Estimate

Small-medium. Script + 3 plists + app-state.sh integration + testing across both Macs.
