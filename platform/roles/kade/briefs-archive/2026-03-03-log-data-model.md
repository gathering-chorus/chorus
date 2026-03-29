# Brief: Log Data Model — Awareness + Potential App-Side Work

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-03
**Card:** N/A — informational + potential work items

## Context

Jeff and I mapped the full logging topology. Two reference docs now live in `data/about/`:
- `LOG_TOPOLOGY.html` — inventory of every log destination, writers, readers, format, health
- `LOG_RELATEDNESS.html` — OWL-style graph showing data flow

## What You Should Know

The **spine → Loki pipeline works well** — the 4 structured JSONL logs (chorus, permissions, errors, handoffs) all flow through Promtail to Grafana. The app's audit-*.jsonl files also flow correctly.

### Your Domain Items

1. **SOLID audit logs** (`data/audit/audit-YYYY-MM-DD.jsonl`) — 24 daily files, 5.8MB total. These are healthy and flowing to Loki. No action needed, but be aware they exist as a queryable data source if you need to debug auth or pod access issues.

2. **Express app container logs** — collected via Docker API by Promtail. Working fine.

3. **Potential card: log viewer surface.** We now have a clear data model of what logs exist and how they connect. If Jeff wants a `/logs` or `/ops` page in the app showing log health status (similar to how the harvest dashboard works), the data model in LOG_RELATEDNESS.html could inform the schema. Not urgent — just flagging as a possibility that came out of this work.

## Silas Brief

Silas got the heavier brief — the infrastructure gaps are in his domain (Promtail config, dead LaunchAgents, daemon log collection). Your awareness is mainly so you know what's available for debugging and potential future app surfaces.

## Files

- `data/about/LOG_TOPOLOGY.html`
- `data/about/LOG_RELATEDNESS.html`
