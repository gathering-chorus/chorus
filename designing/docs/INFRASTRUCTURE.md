# Infrastructure

**Last updated**: 2026-03-03 by Silas (Architect)

## Overview

All Gathering services run on two Mac minis on the same LAN. The primary Mac hosts all compute — Docker containers managed via docker-compose. The secondary Mac serves media and stores backups. Lifecycle is scripted (`app-state.sh`), and observability is built in (Prometheus, Grafana, Loki).

## Service Inventory

### Application Stack (Primary Mac)

| Service | Port | Binding | Runtime | Purpose |
|---------|------|---------|---------|---------|
| Express App | 3000 | 0.0.0.0 | Docker (node:20-alpine) | Main application — UI, API, SOLID pods |
| Apache Fuseki | 3030 | 127.0.0.1 | Docker | SPARQL triplestore — TDB2 persistent, 2GB heap |
| CSS (Community Solid Server) | 3001 | 127.0.0.1 | Docker | Local OIDC provider — client credentials grant (#685) |
| Navidrome | 4533 | 0.0.0.0 | Docker | Self-hosted music streaming (Subsonic API) |
| WebVOWL | 8089 | 127.0.0.1 | Docker | Ontology visualization |
| Vikunja | 3456 | 127.0.0.1 | Docker | Kanban board (2 projects: Gathering, Chorus) |

### Observability Stack (Primary Mac)

| Service | Port | Binding | Purpose |
|---------|------|---------|---------|
| Grafana | 3100 | 127.0.0.1 | Dashboards and alerting (10+ dashboards) |
| Prometheus | 9090 | 127.0.0.1 | Metrics collection (34 alert rules) |
| Alertmanager | 9093 | 127.0.0.1 | Alert routing + grouping |
| Loki | 3102 | 127.0.0.1 | Log aggregation (structured JSON) |
| Promtail | 9080 | internal | Log collection — containers + host logs + daemon logs |
| Node Exporter | 9100 | 127.0.0.1 | Host metrics (CPU, memory, disk, network) |
| Blackbox Exporter | 9115 | 127.0.0.1 | HTTP + ICMP probes (22 LAN devices) |
| mysqld Exporter | 9104 | 127.0.0.1 | WordPress MySQL metrics |

### Content Stack (Primary Mac, currently stopped)

| Service | Port | Binding | Purpose |
|---------|------|---------|---------|
| WordPress | 8081 | 127.0.0.1 | Blog CMS |
| MySQL | 3306 | 127.0.0.1 | WordPress database |

### Secondary Mac (M2 Pro, 3rd floor)

| Service | Purpose |
|---------|---------|
| images-api | Serves ~178TB video/photo library from external drives |

**Total containers**: 18 on primary Mac (M1, 16GB RAM)

### Network Security (ADR-012)

All non-application Docker services are bound to `127.0.0.1` (localhost only). Two services are LAN-accessible: Express app (port 3000, has SOLID OIDC auth) and Navidrome (port 4533, for iOS Subsonic clients).

## Networking

### Docker Networks

- **app-network**: Internal bridge network for application containers
- **observability-network**: Shared bridge network — all projects connect here for metrics/logs

Service-to-service communication uses Docker DNS on IPv4. The Express app listens on `::` for dual-stack (IPv4 + IPv6) browser access.

### Home Network (192.168.86.0/24)

22 devices monitored via ICMP probes (Blackbox Exporter). Both Macs on wired Gigabit Ethernet (1-3ms latency). Dashboard: Grafana → Home Network.

## Deploy Pipeline

Deployment uses docker-compose (migrated from Terraform). `app-state.sh deploy` builds a new image with the current `dist/` and recreates the container.

- **Deploy time**: ~24s average (down from 121s after build cache optimization, 2026-03-03)
- **SHA guard**: Skips rebuild if image already matches current git SHA (bypass with `DEPLOY_FORCE=1`)
- **Bind mounts**: `views/` and `public/` are live — changes appear without deploy. Only `src/` changes require deploy.

### Deploy Lock

File-based deploy locking (`.deploy.lock`) prevents concurrent deploys. PID validation, stale detection, re-entrant for the same process.

## Lifecycle Management

All service lifecycle operations go through `app-state.sh`:

```bash
./app-state.sh start      # Start all stacks in dependency order
./app-state.sh stop       # Graceful shutdown
./app-state.sh restart    # Stop + start
./app-state.sh status     # Container and port health
./app-state.sh logs       # Tail application logs
```

**Rule**: Never use `docker stop/rm/kill` directly. Never kill PIDs manually. All lifecycle through `app-state.sh`.

For multi-stack lifecycle (all 5 stacks):

```bash
system-state.sh status    # All stacks: app, observability, wordpress, bridge, vikunja
system-state.sh health    # Health endpoint probes
system-state.sh verify    # ADR-012 binding verification
```

## Observability

### Grafana Dashboards (localhost:3100)

| Dashboard | UID | What It Shows |
|-----------|-----|---------------|
| Home Cloud | `home-cloud` | CPU, memory, disk, network, service health, constraint gauges |
| Home Network | `home-network` | 22 LAN devices — ICMP status, latency, room-grouped view |
| App Operations | `app-operations` | Application probes, endpoint routing, log streams |
| Alerts Overview | `alerts-overview` | All 34 alert rules — domain filter (app/infra/home) |
| Chorus Activity | `chorus-activity` | Team coordination events, card-commit linking |
| Cost Dashboard | `cost-dashboard` | Claude API + Twilio SMS cost tracking |
| Docker Containers | `docker-containers` | Per-container resource usage |
| Node Metrics | `node-metrics` | Host-level metrics from Docker VM |
| Logs Explorer | `logs-explorer` | Full-text log search via Loki |
| Log Topology | `log-topology` | Log data model — sources, pipelines, gaps |
| Service Overview | `service-overview` | High-level service status |

### Structured Logging

All containers emit structured JSON logs collected by Promtail into Loki. Required fields: `timestamp`, `level`, `appName`, `component`, `domain`, `message`. Queryable via LogQL in Grafana Explore (Loki data source).

Promtail scrape targets:
- **Docker containers**: via Docker SD, filtered to `observability-network` (drops internal obs containers)
- **Chorus operations**: `chorus.log` — team audit events (JSON)
- **Command errors**: `command-errors.log` — Bash failure introspection (JSON)
- **Handoffs**: `handoffs.log` — role-to-role brief exchange (JSON)
- **Permission prompts**: `permission-prompts.log` — tool call tracking (JSON)
- **Daemon logs**: `ops-agent.log`, `defect-poller.log`, `fuseki-perf.log` — LaunchAgent pollers (plain text via `/tmp` bind mount)
- **SOLID audit**: `audit-*.jsonl` — user access events

### Alerting

34 Prometheus alert rules across 3 domains (app: 19, infra: 14, home: 1). Routed through Alertmanager to `alert-notifier.py` (LaunchAgent) → macOS banner notifications. No modals, batched, dashboard link in body. Deploy-tolerant thresholds: ServiceDown/EndpointDown at 5m `for` duration (deploy window ~2.5m).

## Data Persistence

| Data | Storage | Backup |
|------|---------|--------|
| SOLID pods (Turtle/RDF) | Filesystem (`data/pods/`) | Daily cron — 7 daily + 4 weekly rotation |
| Fuseki TDB2 | Docker volume (`fuseki-data`) | Included in daily backup |
| Ontology files | Filesystem (`data/ontology/`) | Included in daily backup |
| Vikunja DB | Docker volume (SQLite) | **Not automated** — known gap |
| Chorus index | `~/.chorus/index.db` (SQLite) | **Not automated** — regenerable from sources |
| WordPress | Docker volume (MySQL) | **Not automated** |

Backup destination: Mac mini M2 Pro at `/Volumes/VideosNew/Gathering/backups/`. Restore verification runs automatically.

## Authentication

### Human Auth
Local CSS (Community Solid Server) via client credentials grant — server-side, no browser redirect, ~91ms warm (#685). Fallback: external OIDC via solidcommunity.net (Pivot). Session persistence via SQLite store with `SESSION_SECRET`. See `SOLID-AUTHENTICATION.md` for full details.

### AI Agent Auth (Service Tokens)
AI roles (Wren, Silas, Kade) authenticate via `POST /api/auth/service-token` → HS256 JWT with WebID claim (1-hour expiry). Access pods via `GET/PUT/DELETE /api/service/pods/:podId/:resourcePath` with ACL enforcement. Agent WebIDs at `/pods/jeff/_agents/{role}/profile/card.ttl#me`.

## Operational Scripts

| Script | Location | Purpose |
|--------|----------|---------|
| `app-state.sh` | App repo root | Application lifecycle (start/stop/restart/status/logs) |
| `system-state.sh` | `messages/scripts/` | Multi-stack lifecycle + ADR-012 verification |
| `cost-report.sh` | `messages/scripts/` | Claude API + Twilio SMS cost tracking |
| `chorus-audit.sh` | `messages/scripts/` | Gate registry + fitness function audit |
| `fuseki-maintenance.sh` | `architect/scripts/` | Fuseki health check, compact, graph cleanup |
| `seed-css.sh` | App `scripts/` | CSS account + pod + client credentials provisioning |

## Hard Constraints

See `infrastructure-constraints.md` for the full set (C1-C7). Key ones:

- **C1**: All application I/O stays on local SSD (no network storage for runtime data)
- **C2**: Primary Mac disk must stay below 95% utilization (raised from 85%)
- **C3**: Docker container count is bounded (18 on 16GB RAM)
- **C6**: No cloud dependencies for core functionality
- **C7**: Harvesters must estimate disk impact before running
