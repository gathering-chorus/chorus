# Brief: Value Stream Quality Alerts Dashboard

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-23
**Card:** #249
**Priority:** P1

## What

A single Grafana dashboard where Jeff can see and filter all card quality gate warnings across the value stream. Same horizontal ops pattern as cost dashboard.

## Data Source

board-ts CLI now emits `quality_gate_warn` events to chorus.log (shipped today, C#57 Phase 1). Structured JSON:

```json
{
  "timestamp": "2026-02-23T...",
  "level": "info",
  "appName": "board-client",
  "component": "cli",
  "event": "quality_gate_warn",
  "role": "wren",
  "card_id": "57",
  "gate": "description_empty",
  "stage": "directing",
  "board": "gathering"
}
```

These already flow to Loki via Promtail. No new infrastructure.

## Gates Currently Emitting

| Gate | Stage | Trigger |
|------|-------|---------|
| `title_short` | Capturing | `board-ts add` with title <10 chars |
| `description_empty` | Directing | `board-ts move <id> Now` with no description |
| `no_comments` | Proving | `board-ts done <id>` with zero comments |

More gates coming in Phase 2 (Kade — Building) and Phase 3 (you — Proving). The dashboard should be designed to pick up new gate types automatically via Loki label extraction.

## Dashboard Layout

Jeff's ask: "one way to consume and filter." Suggested layout:

1. **Overview row** — total warnings (stat), warnings by stage (bar), warnings by role (pie)
2. **Per-stage panels** — one panel per vertebra showing recent warnings with card links
3. **Filters** — role, product, gate type, time range (standard Grafana variables)

Loki base query:
```
{appName="board-client"} |= "quality_gate_warn" | json
```

## Sequencing

This is a companion to #247 (build cycle instrumentation) — same pipeline (chorus.log → Loki → Grafana), different view. Ship them in whichever order makes sense for you. Both feed Jeff's visibility into the value stream.

## Alert Routing

Jeff approved: same alert mechanism as ops. For persistent violations (e.g., card in Now >48h with empty description), route through Alertmanager → osascript (macOS native, already shipped #202). The dashboard is the consumption view; alerts are the push notification.

---
*Wren | PM*
