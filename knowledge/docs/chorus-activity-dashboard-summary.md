# Chorus Activity Dashboard — Work Summary

**Author:** Silas (Architect + Operations)
**Date:** 2026-02-19
**Sessions:** 2 (Feb 18 evening + Feb 19 continuation)

---

## What Was Built

### 1. Structured Logging Pipeline

**Problem:** Operational data (session audits, guardrail blocks, disk checks) existed only as terminal output — ephemeral, unsearchable, invisible to Jeff between sessions.

**Solution:** Three-layer pipeline:

```
Scripts (chorus-audit.sh, infra-guardrails.sh, chorus-log.sh)
    ↓ structured JSON
messages/logs/chorus.log
    ↓ file scraping
Promtail → Loki → Grafana
```

**What changed:**

| File | Change |
|------|--------|
| `messages/scripts/chorus-audit.sh` | Added `log_json()` for every check result, `log_event()` for session lifecycle, disk health check at session start |
| `engineer/.claude/hooks/infra-guardrails.sh` | Added `log_guardrail()` — every blocked/flagged command logged with pattern and command text |
| `messages/scripts/chorus-log.sh` | **NEW** — Standalone event emitter. Usage: `chorus-log.sh <event> <role> [key=value ...]` |
| `shared-observability/config/promtail/promtail-config.yaml` | Added `chorus-operations` scrape job with label promotion (role, level, appName, component) |
| `shared-observability/docker-compose.yml` | Added `messages/logs:/host-logs:ro` volume mount to Promtail |

### 2. Grafana Dashboard

**File:** `shared-observability/dashboards/chorus-activity.json`
**URL:** http://localhost:3100/d/chorus-activity
**Auto-refresh:** 30 seconds

#### Row 1: Team Activity Timeline
- Full reverse-chronological log of all team events
- Shows: session starts/ends, gate checks, guardrail blocks, brief reads
- **How to use:** Scan first when you sit down. "What happened since I last looked?"

#### Row 2: Role Activity (Last 24h)
- Three stat boxes: **Silas** (blue), **Wren** (purple), **Kade** (green)
- Shows event count per role in the selected time range
- **How to use:** DEC-022 glance. Are role ratios near 60/25/15? Is anyone at zero?

#### Row 3: Gate Health
- Left: Stacked bar chart — info (green), warn (yellow), error (red) over time
- Right: Log of actual warning and failure messages
- **How to use:** Any red = gate failed, read right panel for details. Yellow trending up = drift.

#### Row 4: Infrastructure Guardrails
- Left: Count of blocked commands
- Right: Log of each interception (pattern, command)
- **How to use:** Ideally quiet (zero blocks = team using right tools). High count = training signal.

#### Row 5: Session History
- Timeline of session_start and session_end events with role
- **How to use:** Attendance record. Missing session_end = unclean close. Short sessions = blockers.

#### Row 6: Infrastructure Health (DEC-022)
- Disk gauge (green < 75%, yellow 75-90%, red > 90%)
- Sessions today count
- Disk health history log
- **How to use:** Crisis prevention. The Feb 17 disk-full event would have been caught here.

### 3. Logging Contract

Added to `system-architecture.md` — required fields for all structured logs:

| Field | Purpose |
|-------|---------|
| timestamp | ISO 8601 UTC |
| level | info, warn, error |
| appName | Source system (chorus-audit, infra-guardrails, etc.) |
| component | Subsystem (lifecycle, hook, start, close, full) |
| domain | Business domain (Books, Music, Photos, etc.) |
| action | What happened |
| resourceUri | Instance-level traceability |
| correlationId | Cross-system trace |
| message | Human-readable description |

---

## How to Consume the Dashboard

### Morning Check (2 minutes)
1. Open http://localhost:3100/d/chorus-activity
2. Row 2 — who worked? (role stat boxes)
3. Row 6 — disk gauge green?
4. Row 3 right panel — any failures?
5. If all green/quiet, you're clear.

### Between Sessions (30 seconds)
- Glance at Row 1 timeline — anything new?

### Weekly Review
- Change time picker to "Last 7 days"
- Row 2 trends — are role ratios matching DEC-022?
- Row 3 trend — is it getting greener?
- Row 4 trend — approaching zero blocked commands?

---

## What's Not Working Yet

| Panel | Issue | Fix |
|-------|-------|-----|
| Docker container logs | Promtail Docker SD job configured but not ingesting (only `chorus-operations` job active) | Fix network filter — too aggressive, drops everything |
| Wren role stat | Shows 0 | Expected — no Wren sessions have emitted chorus-log events yet |

---

## Architecture Pattern

Jeff recognized this during the session:

> "A little like the way we normalized RDF I/O to a single service"

**Fuseki** = single gateway for RDF knowledge (all domains write Turtle, all queries go through SPARQL)
**Loki** = single gateway for operational data (all scripts emit JSON, all queries go through LogQL)

Same gateway pattern, different data domains. One for what the system *knows*, one for what the system *does*.

---

## What's Next

1. ~~**Fix Docker log ingestion**~~ **DONE** — 9 app containers flowing to Loki (app, fuseki, webvowl, wordpress, mysql, vikunja, slack-bridge). Observability containers filtered via match pipeline drop.
2. ~~**Event emitters**~~ **DONE** — board.sh + chorus-board.sh emit card events. Git post-commit hook emits commit + brief_written events.
3. **Cost tracking** — Add cost to session_end events, show DEC-022 time allocation as pie chart
4. ~~**Alerting**~~ **DONE** — Prometheus: DiskSpaceWarning at 85%. Grafana alerting: no-sessions-48h, gate-failure-streak. Slack notification config pending (manual Grafana UI setup).
5. ~~**Response brief shipped to Wren**~~ **DONE**

---

## Files Changed (All Sessions)

### gathering-team repo (committed: `c7a494c`)
- `messages/scripts/chorus-audit.sh` — structured logging + disk check + lifecycle events
- `messages/scripts/chorus-log.sh` — standalone event emitter
- `messages/scripts/chorus-board.sh` — Chorus Vikunja board wrapper
- `messages/scripts/board.sh` — added chorus_event() emitters
- `engineer/.claude/hooks/infra-guardrails.sh` — structured logging
- `architect/system-architecture.md` — logging contract
- `architect/chorus/chorus-activity-dashboard-summary.md` — this doc
- `architect/briefs/2026-02-19-chorus-activity-dashboard.md` — Wren's brief
- `product-manager/briefs/2026-02-19-chorus-activity-dashboard-response.md` — response to Wren
- `product-manager/chorus-overview.md` — Chorus overview page
- `product-manager/stories.md` — 2 Jeff stories (legibility, Staples invoices)
- `architect/chorus/gate-registry.md` — G4 updated with WordPress containers
- `messages/activity.md` — all actions logged
- `messages/logs/chorus.log` — log file (created)

### shared-observability repo (committed: `d1c11e1`)
- `config/promtail/promtail-config.yaml` — Docker SD + chorus-operations scrape jobs
- `docker-compose.yml` — Promtail port 9080 + host-logs volume mount
- `dashboards/chorus-activity.json` — the dashboard (18 panels, 6 rows)
- `config/prometheus/rules/common-alerts.yml` — DiskSpaceWarning at 85%
- `config/grafana/provisioning/alerting/chorus-alerts.yaml` — Loki-based alert rules

---

## Key Jeff Quotes from These Sessions

> "I like how this is legible — the CEO I worked with at my last job frequently focused on order of operations and legibility — I can see what steps we did rather than hoping we did them"

> "If data isn't in Loki — let's make it flow in"

> "Making our working auditable and legible via automation lets us focus on value demand"

> "A little like the way we normalized RDF I/O to a single service"
