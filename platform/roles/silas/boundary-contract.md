# Silas Boundary Contract

Last updated: 2026-02-19
Owner: Silas (Architect)

Files and formats that other roles depend on from Silas's scope. Changes to these require a `[boundary]` tag in the commit message and a signal to affected roles.

---

## Files Kade Depends On

| File | What Kade uses it for | Break risk |
|------|----------------------|------------|
| `shared-observability/docker-compose.yml` | App stack joins `observability-network` defined here. Promtail collects app logs. | Network rename or removal breaks app metrics/logs |
| `shared-observability/config/prometheus/prometheus.yml` | Scrape job names used in Grafana dashboards Kade built | Job rename breaks dashboard queries |
| `shared-observability/config/prometheus/rules/*.yml` | Alert rules that fire for app issues | Broken rule syntax silently disables all alerts |
| `shared-observability/config/blackbox/blackbox.yml` | HTTP probe modules used by App Operations dashboard | Module change breaks service health panels |
| `architect/ontology/*.ttl` | Class names, property names, namespace URIs used in code | Renamed class/property breaks SPARQL queries and Turtle generation |
| `architect/adr/*.md` | Architectural constraints Kade builds within | Changed constraint without signal = Kade builds against stale rules |
| `architect/infrastructure-constraints.md` | Hard constraints (C1-C7) referenced before adding services/data | Changed constraint without signal = wrong capacity assumptions |

## Files Wren Depends On

| File | What Wren uses it for | Break risk |
|------|----------------------|------------|
| `messages/vikunja/docker-compose.yml` | Kanban board service | Restart/config change = brief board outage |
| `architect/ontology-status.md` | Current ontology state for product decisions | Stale info = wrong product assumptions about what the system can do |
| `architect/service-manifest.md` | Reference for what services exist, their ports, owners | Stale info = wrong operational assumptions |
| `architect/infrastructure-constraints.md` | Constraints that bound product scope | Changed constraint without signal = Wren plans work that violates constraints |

## Files Both Depend On

| File | Used for | Break risk |
|------|----------|------------|
| `messages/scripts/system-state.sh` | Operational commands (status, health, verify) | Broken script = can't check system state |
| Port assignments (port map in service-manifest.md) | All scripts, board.sh, health checks reference specific ports | Port change without updating scripts = silent failures |

## Formats (implicit contracts)

| Format | Who depends on it | Example |
|--------|-------------------|---------|
| Prometheus job names | Kade (Grafana dashboards) | `blackbox-app`, `blackbox-fuseki` — used in panel queries |
| Alert label conventions | Alertmanager routing, Slack notifications | `severity: critical/warning`, `category: compute/network` |
| Ontology namespace URIs | Kade (SPARQL queries, Turtle generation) | `https://jeffbridwell.com/chorus#`, `https://jeffbridwell.com/building#` |
| Docker network name | Kade (Terraform config) | `observability-network` — hardcoded in app Terraform |

---

## What I Broke Today (retrospective)

Restarted observability stack 3 times, vikunja once. Each restart:
- ~30s gap in log collection (Promtail down)
- ~30s board outage (Vikunja down)
- ~15s Loki warm-up (ingester not ready)
- Grafana briefly unavailable

No permanent damage, but no signal sent to Wren or Kade either. They'd discover the gaps on their next session start if they checked log continuity.

---

— Silas
