# Response: Chorus Team Activity Dashboard

**From:** Silas (Architect + Operations)
**To:** Wren (PM)
**Date:** 2026-02-19
**Re:** Your brief at `architect/briefs/2026-02-19-chorus-activity-dashboard.md`
**Chorus Board:** Card #8

---

## Status: SHIPPED

Dashboard is live at: http://localhost:3100/d/chorus-activity

File: `shared-observability/dashboards/chorus-activity.json` (auto-provisioned by Grafana, 30s refresh).

---

## What Was Built

### 6 Rows, 18 Panels

| Row | Panels | Data Source | Purpose |
|-----|--------|-------------|---------|
| **Team Activity Timeline** | 1 (logs) | Loki | Scrolling feed of all team events |
| **Role Activity (24h)** | 3 (stat) | Loki | Event counts per role — Silas/Wren/Kade |
| **Gate Health** | 2 (timeseries + logs) | Loki | Audit pass/warn/fail trend + failure details |
| **Infrastructure Guardrails** | 2 (stat + logs) | Loki | Blocked command count + event log |
| **Session History** | 1 (logs) | Loki | Session start/end timeline |
| **Infrastructure Health** | 3 (gauge + stat + logs) | Prometheus + Loki | Disk %, sessions today, disk history |

### Supporting Infrastructure Built

1. **`chorus-log.sh`** — Standalone event emitter for any script. Usage: `chorus-log.sh <event> <role> [key=value ...]`
2. **Structured JSON logging** — `chorus-audit.sh` and `infra-guardrails.sh` now emit JSON to `messages/logs/chorus.log`
3. **Promtail file scraping** — New `chorus-operations` scrape job reads host-level logs and ships to Loki with label promotion (role, level, appName, component)

---

## Answers to Your Open Questions

### Q1: What's already queryable?

**Now queryable in Loki:**
- Session start/end events (via `chorus-audit.sh` lifecycle hooks)
- All gate check results (pass/warn/fail with role, component, message)
- Infrastructure guardrail blocks (denied commands with pattern name)
- Disk health checks (at every session start + full audit)

**Not yet queryable:**
- App request logs (Promtail Docker SD job isn't ingesting — the observability-network filter + component drop rule is too aggressive. Next step: fix the Docker log pipeline)
- Fuseki query logs (inside Fuseki container, not structured JSON)
- Brief lifecycle events (no automated logging yet — needs emitters in brief workflow)
- Card state changes (Vikunja API doesn't push events — needs polling or hook)

### Q2: Should activity.md updates emit structured logs?

**Not yet, but yes eventually.** The right pattern is: chorus-log.sh emits the event, and the activity.md append is the human-readable record. Same action, two outputs — one for machines (Loki), one for humans (markdown). This avoids double-writing and keeps the sources aligned.

For now, the session start/end hooks emit to Loki automatically. Brief writes and card moves should get emitters next.

### Q3: Real-time or periodic?

**Real-time is free** — Grafana auto-refreshes every 30s, Promtail ships logs within 1-3 seconds of file write. Jeff gets a live-updating dashboard with no polling infrastructure to maintain.

---

## What's Next

1. **Fix Docker container log pipeline** — Promtail is configured but not ingesting Docker logs (only chorus-operations). Likely the observability-network filter is too narrow. Once fixed, we get app request logs in Loki.
2. **Add event emitters** — Wire `chorus-log.sh` into brief write workflow, board.sh card moves, git post-commit hook. Each emitter adds a panel's worth of data.
3. **Time allocation tracking** — Once session start/end events include cost, we can show DEC-022 ratio (60% Wren / 25% Silas / 15% Kade) as a pie chart.
4. **Alerting** — Once data is stable, add Grafana alert rules (disk > 85%, no sessions in 48h, gate failure streak).

---

## Acceptance Criteria Check

From your brief:

- [x] Jeff can see what each role did in the last 24h (Row 1 + Row 2)
- [x] Whether gates are healthy (Row 3)
- [x] Whether infrastructure is stable (Row 6)
- [x] Session history with approximate time allocation (Row 5 — cost tracking pending)

**Card #8 can move to Done** once Jeff confirms the dashboard meets his needs.

---

-- Silas
