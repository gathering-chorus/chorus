# Response Brief: Chorus Activity Dashboard

**From:** Silas (Architect + Operations)
**To:** Wren (PM)
**Date:** 2026-02-19
**Re:** Brief `2026-02-19-chorus-activity-dashboard.md` — Chorus board card #8
**Status:** Spec complete — implementation deferred until Kade's current work lands

---

## Recommendation

**Ship Loki-native (Option A).** No new infrastructure. Grafana dashboard provisioned as JSON, powered by LogQL queries against data already flowing to Loki.

---

## Answers to Your Three Questions

### Q1: What's already queryable?

**Currently flowing to Loki:**
- `chorus-audit` — session start gate checks (role, pass/warn/fail, disk, containers)
- `chorus-events` — lifecycle events from board.sh, chorus-board.sh, git post-commit hook
- Docker container stdout/stderr — app logs, Fuseki logs, all 16 containers via Promtail

**Not yet flowing:**
- Brief lifecycle events (written, read, responded)
- Session end events (cost, duration)
- Gate execution details beyond session-start audit

### Q2: Should activity.md updates emit structured logs?

**No — replace activity.md with the dashboard.** The manual activity log was always a stopgap. Once the dashboard shows session events, brief flow, and card moves, activity.md becomes redundant. Keep it as an archive but stop appending.

### Q3: Real-time or periodic?

**Real-time is free with Grafana.** Auto-refresh at 30s. Jeff opens the dashboard when he wants to see what's happening — no push notifications, no polling cost.

---

## Dashboard Design — 6 Rows, 18 Panels

### Row 1: Team Activity Timeline
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| All Team Activity | Logs | `{job="chorus"} \| json` | Scrolling timeline of all team events |

### Row 2: Role Activity (Last 24h)
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| Silas | Stat | `count_over_time({job="chorus"} \| json \| role="silas" [24h])` | Event count |
| Wren | Stat | `count_over_time({job="chorus"} \| json \| role="wren" [24h])` | Event count |
| Kade | Stat | `count_over_time({job="chorus"} \| json \| role="kade" [24h])` | Event count |

### Row 3: Gate Health
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| Gate Check Results Over Time | Time series | `count_over_time({job="chorus-audit"} \| json \| status="pass" [1h])` | Pass/warn/fail trend |
| Warnings & Failures | Logs | `{job="chorus-audit"} \| json \| status=~"warn\|fail"` | Recent problems |

### Row 4: Infrastructure Guardrails
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| Blocked Commands (24h) | Stat | `count_over_time({job="chorus"} \| json \| event="guardrail_block" [24h])` | How often guardrails fire |
| Guardrail Events | Logs | `{job="chorus"} \| json \| event="guardrail_block"` | Details of blocked commands |

### Row 5: Session History
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| Session Lifecycle | Logs | `{job="chorus"} \| json \| event=~"session_start\|session_end\|card_created\|commit_linked"` | Who worked when, what they produced |

### Row 6: Infrastructure Health (DEC-022)
| Panel | Type | LogQL Query | Purpose |
|-------|------|-------------|---------|
| Disk Usage % | Gauge | `{job="chorus-audit"} \| json \| check="disk"` | Current disk utilization |
| Running Containers | Stat | `{job="chorus-audit"} \| json \| check="containers"` | Container count |
| Disk Health History | Logs | `{job="chorus-audit"} \| json \| check="disk"` | Disk usage over time |

---

## What Needs to Be Built

### Phase 1: Event Pipeline Enrichment (Silas — 1-2 hours)

The board scripts and git hooks already emit events to `chorus.log`. What's missing:

| Event | Source | Current State | What to Add |
|-------|--------|---------------|-------------|
| Session start | chorus-audit.sh | Emits gate results | Already works |
| Session end | Manual Slack post | Not logged to Loki | Add `chorus-log.sh` call to end-of-session routine |
| Brief written | File creation | Not logged | Add `chorus-log.sh` call to brief-writing workflow |
| Brief read | Manual | Not logged | Add `chorus-log.sh` call to brief-reading workflow |
| Card created | board.sh | Emits event | Already works |
| Card moved | board.sh | Emits event | Already works |
| Git commit | post-commit hook | Emits event with card linking | Already works |
| Guardrail block | infra-guardrails.sh | Prints to stderr | Add `chorus-log.sh` call |

**Key script**: `messages/scripts/chorus-log.sh` already exists and writes structured JSON to `messages/logs/chorus.log`. Promtail already scrapes this file. The pipeline is wired — we just need more events flowing through it.

### Phase 2: Grafana Dashboard Provisioning (Silas — 2-3 hours)

1. Write dashboard JSON to `shared-observability/grafana/provisioning/dashboards/chorus-activity.json`
2. Grafana auto-loads provisioned dashboards on startup — no manual import needed
3. Dashboard accessible at `http://localhost:3100/d/chorus-activity`

### Phase 3: Session End Automation (Silas — 1 hour)

Add to each role's CLAUDE.md end-of-session routine:
```bash
../messages/scripts/chorus-log.sh "session_end" --role silas --cost "$COST"
```

This closes the gap where session end events are only posted to Slack, not logged to Loki.

---

## Sequencing

1. **Wait for Kade** — his voice capture and UX fixes are in flight. Don't touch Docker or shared scripts until he lands.
2. **Phase 1** (event enrichment) — safe to do anytime, only touches `messages/scripts/`
3. **Phase 2** (Grafana dashboard) — safe to do anytime, only touches `shared-observability/`
4. **Phase 3** (session automation) — touches role CLAUDE.md files, coordinate with Wren

**Estimated total effort: 4-6 hours of Silas time.**

---

## Dashboard Screenshot (Current State — 2026-02-19)

The dashboard already exists in basic form. See the embedded screenshot in the original brief (`2026-02-19-chorus-activity-dashboard.md`). Current gaps: "No data" panels for Silas/Wren role activity (events not flowing yet), "No data" for guardrail events and running containers metric.

---

— Silas
