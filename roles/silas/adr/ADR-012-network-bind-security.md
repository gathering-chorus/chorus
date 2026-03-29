# ADR-012: Bind Docker Services to 127.0.0.1

**Date**: 2026-02-19
**Status**: **Implemented** — Kade committed config changes (2026-02-19), Silas activated via stack restarts (2026-02-19)
**Decider**: Jeff (directive), Silas (architecture)
**References**: Network inventory (`network-inventory.md`), Secrets audit (`briefs/2026-02-16-secrets-audit.md`), ADR-011 (deployment pattern)

## Context

All 16 Docker containers on the primary Mac (192.168.86.36) bind their ports to `0.0.0.0`, making every service accessible from any device on the LAN (192.168.86.0/24 — 22 devices including iPhones, Apple TVs, HomePods, printer, etc.).

Most of these services have no authentication:
- **Fuseki** (3031) — SPARQL endpoint, full read/write to the knowledge graph
- **Prometheus** (9090) — metrics, targets, configuration visible
- **Grafana** (3100) — dashboards, Loki queries (password was `admin` until secrets audit)
- **Loki** (3102) — raw log queries
- **MySQL** (3306) — WordPress database, direct SQL access
- **Vikunja** (3456) — kanban board
- **Node Exporter** (9100) — full host metrics (CPU, disk, memory, network)
- **Alertmanager** (9093) — alert configuration
- **MailHog** (8025) — captured emails
- **WebVOWL** (8089) — ontology visualization

The only service that **should** be LAN-accessible is the main app on port 3000 (it has SOLID auth).

This is a P1 security issue. While the LAN is trusted (home network, WPA3), any compromised device becomes a vector to the entire infrastructure.

## Decision

Bind all non-app Docker services to `127.0.0.1` (localhost only). The main app on port 3000 stays on `0.0.0.0` for LAN access.

## Implementation Plan

### Files to Change

#### 1. Terraform — personal-site stack
**File**: `jeff-bridwell-personal-site/terraform/environments/dev/main.tf`
```terraform
# App container — KEEP on 0.0.0.0 (has auth, needs LAN access)
ports {
  internal = var.app_port
  external = var.app_port
  ip       = "0.0.0.0"    # Intentionally LAN-accessible
  protocol = "tcp"
}
```

**File**: `jeff-bridwell-personal-site/terraform/environments/dev/fuseki.tf`
```terraform
# Fuseki — CHANGE to 127.0.0.1
ports {
  internal = 3030
  external = var.fuseki_port
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}
```

**File**: `jeff-bridwell-personal-site/terraform/environments/dev/webvowl.tf`
```terraform
# WebVOWL — CHANGE to 127.0.0.1
ports {
  internal = 8080
  external = var.webvowl_port
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}
```

**File**: `jeff-bridwell-personal-site/terraform/environments/dev/prometheus/main.tf`
```terraform
# Prometheus — CHANGE to 127.0.0.1
ports {
  internal = 9090
  external = 9090
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}

# Grafana — CHANGE to 127.0.0.1
ports {
  internal = 3000
  external = 3100
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}

# Node Exporter — CHANGE to 127.0.0.1
ports {
  internal = 9100
  external = 9100
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}
```

**File**: `jeff-bridwell-personal-site/terraform/environments/dev/prometheus/loki.tf`
```terraform
# Loki — CHANGE to 127.0.0.1
ports {
  internal = 3100
  external = 3102
  ip       = "127.0.0.1"    # Was: "0.0.0.0"
  protocol = "tcp"
}
```

#### 2. Docker Compose — observability stack
**File**: `shared-observability/docker-compose.yml`

Change every `ports:` entry from `"XXXX:XXXX"` to `"127.0.0.1:XXXX:XXXX"`:
```yaml
# Before
ports:
  - "9090:9090"

# After
ports:
  - "127.0.0.1:9090:9090"
```

Services to change: prometheus, alertmanager, grafana, loki, promtail, node-exporter

#### 3. Docker Compose — Vikunja
**File**: `messages/vikunja/docker-compose.yml`
```yaml
ports:
  - "127.0.0.1:3456:3456"    # Was: "3456:3456"
```

#### 4. ~~Docker Compose — Slack Bridge~~ (Retired 2026-03 — Slack deprecated)

#### 5. Terraform — WordPress stack
**File**: `wordpress-blog/terraform/main.tf`

Change all four port bindings:
```terraform
# MySQL — CHANGE to 127.0.0.1
ip = "127.0.0.1"    # Was: "0.0.0.0"

# WordPress — CHANGE to 127.0.0.1
ip = "127.0.0.1"    # Was: "0.0.0.0"

# MailHog SMTP — CHANGE to 127.0.0.1
ip = "127.0.0.1"    # Was: "0.0.0.0"

# MailHog UI — CHANGE to 127.0.0.1
ip = "127.0.0.1"    # Was: "0.0.0.0"
```

### Summary Table

| Service | Port | Current | Target | Config File |
|---------|------|---------|--------|-------------|
| **App** | 3000 | 0.0.0.0 | **0.0.0.0** (keep) | main.tf |
| Fuseki | 3031 | 0.0.0.0 | 127.0.0.1 | fuseki.tf |
| WebVOWL | 8089 | 0.0.0.0 | 127.0.0.1 | webvowl.tf |
| Prometheus | 9090 | 0.0.0.0 | 127.0.0.1 | prometheus/main.tf |
| Grafana | 3100 | 0.0.0.0 | 127.0.0.1 | prometheus/main.tf |
| Loki | 3102 | 0.0.0.0 | 127.0.0.1 | prometheus/loki.tf |
| Node Exporter | 9100 | 0.0.0.0 | 127.0.0.1 | prometheus/main.tf |
| Alertmanager | 9093 | 0.0.0.0 | 127.0.0.1 | docker-compose.yml |
| Promtail | 9080 | 0.0.0.0 | 127.0.0.1 | docker-compose.yml |
| Vikunja | 3456 | 0.0.0.0 | 127.0.0.1 | vikunja/docker-compose.yml |
| ~~Slack Bridge~~ | ~~3460~~ | — | **Retired** | ~~slack-bridge/docker-compose.yml~~ |
| WordPress | 8081 | 0.0.0.0 | 127.0.0.1 | wordpress/main.tf |
| MySQL | 3306 | 0.0.0.0 | 127.0.0.1 | wordpress/main.tf |
| MailHog UI | 8025 | 0.0.0.0 | 127.0.0.1 | wordpress/main.tf |
| MailHog SMTP | 1025 | 0.0.0.0 | 127.0.0.1 | wordpress/main.tf |

### Sequencing

1. **Wait for Kade to land current work** — voice capture and UX fixes touch Dockerfile and app-state.sh
2. **Apply all changes in one pass** — this is a config-only change, no code changes
3. **Restart all stacks** via `app-state.sh deploy` + `docker-compose up -d` for compose stacks
4. **Verify** — `docker ps --format "{{.Ports}}"` should show `127.0.0.1:XXXX->` for everything except app

### Risks

1. **Inter-container communication**: Containers on the same Docker network communicate by container name, not `localhost:port`. Binding to 127.0.0.1 only affects host-side access. Container-to-container traffic is unaffected.
2. **Prometheus scraping**: Prometheus scrapes exporters by container name on the Docker network — no host port needed. Verified: Prometheus config uses `node-exporter:9100`, not `localhost:9100`.
3. **Grafana → Loki**: Same Docker network, uses `http://loki:3100` internally. Host bind doesn't matter.
4. **App → Fuseki**: App connects to Fuseki via Docker network (`fuseki:3030`). Host bind doesn't matter.

**No functional impact expected.** This only changes who can reach services from outside the Docker network.

## Consequences

**Positive:**
- 14 services no longer accessible from LAN
- Eliminates attack surface from compromised IoT devices, guest WiFi, etc.
- Weak/default credentials (MySQL, Grafana pre-audit) no longer exploitable remotely
- Aligns with security best practice for dev infrastructure

**Negative:**
- Can't access Grafana/Vikunja from iPhone or other LAN devices (use Mac directly)
- If Jeff ever wants remote access to dashboards, needs a reverse proxy with auth

**Neutral:**
- No performance impact
- No container-to-container communication changes

---

— Silas
