# Brief: Chorus Team Activity Dashboard

**From:** Wren (PM)
**To:** Silas (Architect + Operations)
**Date:** 2026-02-19
**Priority:** P2 — Next
**Chorus Board:** Card #8
**Re:** Jeff wants a log-level view of team actions and interactions

---

## Current Dashboard (2026-02-19)

![Chorus — Team Activity Dashboard](./chorus-activity-dashboard.png)

---

## What Jeff Asked For

> "maybe silas can wire up a log level view of the chorus team actions and interactions"

Jeff wants to see what the team is actually doing — not just standup summaries, but a live or near-live view of actions, decisions, and interactions across all three roles.

---

## What Already Exists

You've built most of the infrastructure for this:

1. **chorus-audit.sh** — emits structured JSON to `messages/logs/chorus.log`
2. **infra-guardrails.sh** — also logs to `messages/logs/chorus.log`
3. **Promtail** — scrapes `messages/logs/` and ships to Loki
4. **Loki** — queryable via Grafana

What's missing is the **Grafana dashboard** that makes this visible to Jeff without writing LogQL queries.

---

## Suggested Dashboard Panels

### Row 1: Team Activity Timeline
- **Panel type:** Logs or Table
- **Query:** `{appName="chorus-audit"} | json | line_format "{{.timestamp}} [{{.role}}] {{.check}}: {{.message}}"`
- **Purpose:** Scrolling timeline of what each role is doing, when

### Row 2: Role Activity Summary
- **Panel type:** Stat or Bar gauge (3 panels, one per role)
- **Query:** Count of log entries per role in last 24h
- **Purpose:** Quick visual of who's been active

### Row 3: Gate Health
- **Panel type:** Table
- **Query:** Latest gate check results (PASS/WARN/FAIL per gate)
- **Purpose:** Are our quality gates healthy?

### Row 4: Infrastructure Health
- **Panel type:** Stat panels
- **Query:** Disk usage %, container health count, last backup timestamp
- **Purpose:** DEC-022 says you own this — make it visible

### Row 5: Session History
- **Panel type:** Table
- **Query:** Session start/close events with role, duration, cost
- **Purpose:** Track DEC-022 time allocation (60/25/15 target)

---

## Open Questions

1. **What's already queryable?** I know chorus-audit.sh logs gate checks and session events. Are there other log sources that would be useful? (e.g., app request logs, Fuseki query logs)

2. **Should activity.md updates also emit structured logs?** Right now activity.md is a markdown file — not queryable. If roles logged activity entries as JSON too, the dashboard could show the full picture.

3. **Real-time or periodic?** Jeff probably doesn't need a live-updating dashboard (he's not watching it all day). A page he can check once per morning would suffice. But if Grafana's already there, real-time is free.

---

## Acceptance Criteria

- Jeff can open a Grafana dashboard (or a link from the Chorus overview page) and see:
  - What each role did in the last 24h
  - Whether gates are healthy
  - Whether infrastructure is stable
  - Session history with approximate time allocation

---

— Wren

