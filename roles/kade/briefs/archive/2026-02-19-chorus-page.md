# Brief: Chorus Page in Gathering App

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-19
**Priority:** P2 — Next
**Chorus Board:** Card #7
**Re:** Static page for Chorus product with doc links + Loki activity view

---

## What Jeff Wants

A `/chorus` page in the Gathering app that serves two purposes:

1. **Doc links** — a landing page with links to all Chorus artifacts (team architecture, gate registry, ontology, ADRs, briefs, board)
2. **Team activity view** — live or near-live display of Chorus team actions pulled from Loki structured logs

---

## Part 1: Doc Links

A simple reference section linking to the core Chorus documents. Content source: `product-manager/chorus-overview.md` has the full inventory. Key sections:

### Core Documents
| Document | Path |
|----------|------|
| Team Architecture | messages/team-architecture.md |
| Gate Registry | architect/chorus/gate-registry.md |
| Decisions Log | product-manager/decisions.md |
| Chorus Overview | product-manager/chorus-overview.md |

### ADRs
- ADR-009 (pending): Chorus pipeline ontology
- ADR-010: Harvest pipeline + quality gates
- ADR-011: Production-like deployment pattern

### Operational
- Chorus Audit Runner: `messages/scripts/chorus-audit.sh`
- Chorus Board: `messages/scripts/chorus-board.sh`
- Infra Guardrails: `engineer/.claude/hooks/infra-guardrails.sh`

### Team
| Role | Person | Focus |
|------|--------|-------|
| Director | Jeff | Vision + decisions |
| PM | Wren | What + why + when |
| Architect | Silas | How + constraints + ops |
| Engineer | Kade | Build + test + ship |

These can be rendered as static HTML from the markdown, or hardcoded in a template. Keep it simple — this is a reference page, not a dynamic app.

---

## Part 2: Loki Activity View

This is the interesting part. Silas has structured JSON logs flowing to Loki via Promtail:

- **Source file:** `messages/logs/chorus.log`
- **Log format:** JSON with fields: `timestamp`, `level`, `appName`, `component`, `role`, `check`, `message`
- **Loki endpoint:** `http://localhost:3100`

### What to show

**Team Activity Timeline** — a scrolling list of recent team actions:
- Session starts/ends per role
- Gate check results (PASS/WARN/FAIL)
- Container health status
- Disk health checks

**Query approach:**
```
GET http://localhost:3100/loki/api/v1/query_range
  query: {appName="chorus-audit"} | json
  start: <24h ago>
  end: <now>
  limit: 100
```

### Suggested UI

- Simple table or log-style display, newest first
- Color-code by level: green (info/pass), yellow (warn), red (error/fail)
- Filter by role (Wren/Silas/Kade)
- Filter by component (start/close/full)
- Auto-refresh every 60 seconds (or manual refresh button)

### Implementation Notes

- Loki's HTTP API is simple — no client library needed, just fetch + JSON parse
- Query from the server side (Express route) to avoid CORS issues with Loki
- Parse the JSON log lines and render in the template
- No auth needed for Loki (internal network only)

---

## Acceptance Criteria

- [ ] `/chorus` page exists and is accessible from the app (add to Admin menu or utility links)
- [ ] Doc links section renders with working links to key Chorus documents
- [ ] Activity section shows recent chorus-audit log entries from Loki
- [ ] Entries are color-coded by level
- [ ] Filter by role works
- [ ] Page uses gathering.css design tokens
- [ ] Screenshot posted to #kade before declaring done (DEC-022)

---

## What This Is NOT

- Not a full Grafana replacement — just a simple activity view
- Not real-time WebSocket streaming — polling or manual refresh is fine
- Not a dashboard with charts/graphs — that's Grafana's job

---

— Wren
