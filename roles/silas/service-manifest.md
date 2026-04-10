# Service Manifest

Last updated: 2026-02-27
Owner: Silas (Architect)

Services across two machines: **Library** (192.168.86.36) — 15 Docker containers + 10 LaunchAgents. **Bedroom** (192.168.86.242) — 3 LaunchAgent services (bare Node.js).

---

## Gathering App Stack (Docker Compose — ADR-015)

Config: `jeff-bridwell-personal-site/docker-compose.yml`
Lifecycle: `app-state.sh start|stop|restart|deploy|status`

| Container | Port | Bind | Health Endpoint | Health Check | Owner |
|-----------|------|------|-----------------|-------------|-------|
| jeff-bridwell-personal-site-app | 3000 | 0.0.0.0 | /health | Docker HEALTHCHECK | Kade |
| jeff-bridwell-personal-site-fuseki | 3030 | 127.0.0.1 | /$/ping | Docker HEALTHCHECK | Kade |
| jeff-bridwell-personal-site-webvowl | 8089 | 127.0.0.1 | / | Docker HEALTHCHECK | Kade |

**Dependencies**: Fuseki must be up before app starts. App → Fuseki via Docker network (`fuseki:3030`).

---

## Observability Stack (Docker Compose)

Config: `shared-observability/docker-compose.yml`
Lifecycle: `docker compose -f shared-observability/docker-compose.yml up -d` or `system-state.sh start`

| Container | Port | Bind | Health Endpoint | Health Check | Owner |
|-----------|------|------|-----------------|-------------|-------|
| prometheus | 9090 | 127.0.0.1 | /-/ready | wget | Silas |
| grafana | 3100→3000 | 127.0.0.1 | /api/health | wget | Silas |
| loki | 3102→3100 | 127.0.0.1 | /ready | wget (30s start) | Silas |
| promtail | 9080 | 127.0.0.1 | /ready | bash /dev/tcp | Silas |
| alertmanager | 9093 | 127.0.0.1 | /-/ready | wget | Silas |
| node-exporter | 9100 | 127.0.0.1 | /metrics | wget | Silas |
| blackbox-exporter | 9115 | — | / | wget | Silas |
| mysqld-exporter | 9104 | — | /metrics | wget | Silas |

**Dependencies**: Loki before Promtail. Prometheus + Loki before Grafana.
**Network**: All on `observability-network`. Other stacks join this network for metrics/logs.
**Note**: Loki needs ~15s after start for ingester warm-up. Health check has 30s start_period.

---

## WordPress Stack (Terraform)

Config: `wordpress-blog/terraform/main.tf`
Lifecycle: `wordpress-blog/wordpress.sh start|stop|restart`

| Container | Port | Bind | Health Endpoint | Health Check | Owner |
|-----------|------|------|-----------------|-------------|-------|
| wordpress-blog | 8081 | 127.0.0.1 | / | Docker HEALTHCHECK | Kade |
| wordpress-mysql | 3306 | 127.0.0.1 | — | Docker HEALTHCHECK | Kade |
| wordpress-mailhog | 1025, 8025 | 127.0.0.1 | — | **NONE** | Kade |

**Dependencies**: MySQL must be up before WordPress.
**Note**: MailHog is the only container without a health check (Terraform-managed, needs Kade to add).

---

## Vikunja Stack (Docker Compose)

Config: `messages/vikunja/docker-compose.yml`
Lifecycle: `docker compose -f messages/vikunja/docker-compose.yml up -d` or `system-state.sh start`

| Container | Port | Bind | Health Endpoint | Health Check | Owner |
|-----------|------|------|-----------------|-------------|-------|
| vikunja | 3456 | 127.0.0.1 | /api/v1/info | vikunja healthcheck | Silas |

**Storage**: SQLite at `messages/vikunja/db/vikunja.db`. Files at `messages/vikunja/files/`.

---

## Library LaunchAgent Services

Non-Docker services managed by launchd on Library (192.168.86.36).

| Label | Type | Port | Function | Owner |
|-------|------|------|----------|-------|
| `com.chorus.docker-services` | run-once | — | Boot-order orchestration — starts all Docker stacks on reboot (#382) | Silas |
| `com.chorus.api` | KeepAlive | 3340 | Chorus context index HTTP API | Silas |
| `com.chorus.alert-notifier` | KeepAlive | — | macOS desktop alert notifications (banner + Basso sound) | Silas |
| `com.chorus.session-watcher` | KeepAlive | — | Ambient chorus index daemon (fswatch on JSONL) | Silas |
| `com.chorus.defect-poller` | KeepAlive | — | Loki error polling → auto-card creation (5 min interval) | Silas |
| `com.chorus.fuseki-perf` | KeepAlive | — | RDF store performance monitoring | Silas |
| `com.chorus.ops-agent` | KeepAlive | — | Operational health agent | Silas |
| `com.chorus.fuseki-compact` | StartCalendarInterval | — | TDB2 weekly compact, Saturday 1am | Silas |
| `com.chorus.andon-light` | KeepAlive | — | Floating menubar role status display (Swift) | Silas |
| `com.chorus.andon-enrich` | StartInterval (30s) | — | Slow-path enrichment for andon light | Silas |

**Plist location**: `~/Library/LaunchAgents/`

---

## Bedroom Services (192.168.86.242)

Bare Node.js services managed by launchd. NOT Docker.

| Label | Type | Port | Function | Owner |
|-------|------|------|----------|-------|
| `com.gathering.images-api-server` | KeepAlive | 3001 | Gallery UI — browse photos/videos | Kade |
| `com.gathering.images-api-video` | KeepAlive | 8082 | Media serving — stream video/photos | Kade |
| `com.gathering.volume-keepalive` | run-once (4min) | — | USB enclosure idle prevention | Silas |

**Code**: `CascadeProjects/personal-website/` (GitHub: WJeffBridwell/personal-website)
**Logs**: `/tmp/images-api-server.log`, `/tmp/images-api-video.log`
**Deploy**: `messages/scripts/images-api-deploy.sh` (git push → pull → launchctl restart)

---

## Health Check Summary

| Status | Count | Services |
|--------|-------|----------|
| Docker HEALTHCHECK (healthy) | 14 | All Docker except wordpress-mailhog |
| Docker (no health check) | 1 | wordpress-mailhog |
| LaunchAgent (Library) | 10 | All chorus.* agents |
| LaunchAgent (Bedroom) | 3 | All gathering.* agents |

**Verification**: `system-state.sh health` checks HTTP endpoints. `system-state.sh verify` checks ADR-012 port bindings.

---

## Network Topology

All stacks connect to `observability-network` for metrics collection and log shipping.

```
observability-network (Library Docker)
├── prometheus (scrapes all)
├── grafana (queries prometheus + loki)
├── loki (receives logs from promtail)
├── promtail (reads docker.sock)
├── alertmanager
├── blackbox-exporter (HTTP + ICMP probes)
├── node-exporter
├── mysqld-exporter → wordpress-mysql
├── app → fuseki (SPARQL)
├── app → webvowl
├── wordpress → mysql → mailhog
└── vikunja

LaunchAgent services (Library, non-Docker)
├── chorus-api (port 3340)
├── session-watcher (fswatch daemon)
├── alert-notifier (desktop notifications)
├── defect-poller (Loki → auto-card)
├── fuseki-perf (RDF monitoring)
├── fuseki-compact (TDB2 weekly compact)
├── ops-agent (health checks)
├── andon-light (menubar role status)
├── andon-enrich (30s enrichment loop)
└── docker-services (boot orchestration)

Bedroom services (bare Node.js)
├── images-api-server (port 3001)
├── images-api-video (port 8082)
└── volume-keepalive (USB idle prevention)
```

---

## Port Map (quick reference)

### Library (192.168.86.36)

| Port | Service | Bind |
|------|---------|------|
| 3000 | Gathering App | 0.0.0.0 (LAN) |
| 3030 | Fuseki SPARQL | 127.0.0.1 |
| 3100 | Grafana | 127.0.0.1 |
| 3102 | Loki | 127.0.0.1 |
| 3306 | MySQL | 127.0.0.1 |
| 3340 | Chorus API | 127.0.0.1 |
| 3456 | Vikunja | 127.0.0.1 |
| 8025 | MailHog UI | 127.0.0.1 |
| 8081 | WordPress | 127.0.0.1 |
| 8089 | WebVOWL | 127.0.0.1 |
| 9080 | Promtail | 127.0.0.1 |
| 9090 | Prometheus | 127.0.0.1 |
| 9093 | Alertmanager | 127.0.0.1 |
| 9100 | Node Exporter | 127.0.0.1 |

### Bedroom (192.168.86.242)

| Port | Service | Bind |
|------|---------|------|
| 3001 | images-api server (gallery UI) | 0.0.0.0 |
| 8082 | images-api video (media serving) | 0.0.0.0 |

---

— Silas
