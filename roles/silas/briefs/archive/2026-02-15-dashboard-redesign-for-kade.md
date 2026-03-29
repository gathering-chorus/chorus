# Brief: Infrastructure Dashboard Redesign

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P2 — promoted by Jeff (2026-02-15). Glimmer on hold, dashboard is next.
**Depends on**: ~~Style guide CSS~~ UNBLOCKED — gathering.css shipped (all 19 views migrated, design tokens landed).
**Status**: GO — design ready, Kade builds next

---

## Context

Jeff reviewed the App Operations dashboard and gave clear feedback:

1. **The infrastructure diagram is static** — it's a picture, not live data. No health indicators.
2. **The metrics row is meaningless without context** — "CPU Usage: No data" when no service is selected doesn't help anyone.
3. **No click-to-drill-down** — you can't select a piece of infrastructure and see its specific load.

What Jeff wants: **"I want to see correct state of infra (red/green/yellow) + specific load if I select a piece of infra."**

---

## Design Tokens (from Wren's Style Guide)

The dashboard MUST use the same color language as the application:

```
Healthy / Up:     #10B981  (emerald)
Warning / Degraded: #F59E0B  (amber)
Danger / Down:    #EF4444  (red)
Accent / Interactive: #6366F1  (indigo)
Muted / Secondary: #718096
Background:       #F7FAFC
Card:             #FFFFFF
Border:           #E2E8F0
```

These are the Gathering design tokens. Use them for Grafana threshold colors, panel backgrounds, and text colors where possible.

---

## What to Build

### Row 1: Service Health Tiles

Replace the static infrastructure diagram with a row of **Grafana stat panels**, one per service. Each tile shows the service name and health state via color.

| Service | Metric | Thresholds |
|---------|--------|------------|
| Express App | `probe_success{job="blackbox-app"}` | 0 = #EF4444 (red), 1 = #10B981 (green) |
| Fuseki | `probe_success{job="blackbox-fuseki"}` | 0 = #EF4444, 1 = #10B981 |
| WordPress | `probe_success{job="blackbox-wordpress"}` | 0 = #EF4444, 1 = #10B981 |
| WebVOWL | `probe_success{job="blackbox-webvowl"}` | 0 = #EF4444, 1 = #10B981 |
| Vikunja | `probe_success{job="blackbox-vikunja"}` | 0 = #EF4444, 1 = #10B981 |
| Prometheus | `up{job="prometheus"}` | 0 = #EF4444, 1 = #10B981 |
| Grafana | `up{job="grafana"}` | 0 = #EF4444, 1 = #10B981 |
| Loki | `up{job="loki"}` | 0 = #EF4444, 1 = #10B981 |

Display: **stat panel**, value mapping `0 → DOWN`, `1 → UP`. Background color mode so the whole tile is green or red. No graph — just the state.

Layout: arrange in groups matching the current diagram's logic:
- **Application**: App, Fuseki
- **Content**: WordPress
- **Observability**: Prometheus, Grafana, Loki
- **Supporting**: WebVOWL, Vikunja

### Row 2: Service Detail (Variable-Driven)

Update the **Service** template variable so it maps to actual metric label values. When a user selects a service from the dropdown (or clicks a tile if Grafana supports data links to variable updates), the detail row shows that service's metrics.

**For Express App** (the primary service):
- Request rate: `sum(rate(http_request_duration_seconds_count[5m]))`
- Error rate: `sum(rate(http_request_duration_seconds_count{status_code=~"5.."}[5m])) / sum(rate(http_request_duration_seconds_count[5m]))`
- P95 latency: `histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))`
- Active sessions: existing panel
- Top endpoints by request rate (table)

**For Fuseki**:
- Query rate (if metrics available)
- Probe response time: `probe_duration_seconds{job="blackbox-fuseki"}`

**For WordPress**:
- API request count (existing panel)
- Probe response time: `probe_duration_seconds{job="blackbox-wordpress"}`

**For other services**: Show probe response time + uptime percentage over selected time range.

### Remove: Generic Infrastructure Metrics

**Delete** the CPU Usage, Memory Usage, Host CPU Usage panels from this dashboard. These belong in a dedicated infrastructure dashboard.

**Add instead**: Import Grafana dashboard **#1860** (Node Exporter Full) — it's pre-built, comprehensive, and Node Exporter at :9100 is already feeding Prometheus. Add a link from the App Operations dashboard to this imported dashboard for "Host Infrastructure Details."

For container-level metrics, consider adding **cAdvisor** (one additional container, feeds Prometheus natively) with imported dashboard **#893** (Docker monitoring). This is optional — only if Jeff wants per-container CPU/memory drill-down beyond what Node Exporter provides.

---

## What NOT to Build

- Don't recreate the static architecture diagram as a fancy Grafana plugin (FlowChart, Node Graph). Live health tiles are simpler and more useful.
- Don't instrument custom CPU/memory metrics in the app. Node Exporter + cAdvisor handle this.
- Don't build a custom theme for Grafana. Just use the design token hex values in thresholds and overrides.

---

## Acceptance Criteria

1. Opening the dashboard shows at-a-glance health: all green = everything healthy, any red = immediate attention
2. Selecting a service shows that service's specific metrics (not generic host metrics)
3. No "No data" panels visible in normal operation
4. Colors match Gathering design tokens (#10B981 green, #EF4444 red, #F59E0B amber)
5. Link to Node Exporter dashboard for host-level deep dive

---

## Estimate

| Task | Time |
|------|------|
| Health tile stat panels (8 services) | ~30 min |
| Wire Service variable to detail panels | ~20 min |
| Import Node Exporter dashboard #1860 | ~5 min |
| Remove generic metrics panels, add cross-link | ~10 min |
| Color theming with design tokens | ~15 min |
| **Total** | **~1.5 hrs** |

— Silas
