# ADR-005: Observability Evolves with Infrastructure

**Status**: Accepted
**Date**: 2026-02-15
**Decided by**: Jeff Bridwell, Silas (Architect)

## Context

As of Feb 14, the system has grown significantly — WebVOWL (port 8089), SMS capture (Twilio webhook), Vikunja kanban (port 3456), pod backup cron, and new application metrics. The shared-observability stack was last significantly updated when the core services (Express, Fuseki, WordPress) were set up. New infrastructure ships without corresponding observability updates.

This creates blind spots. WebVOWL runs without a health probe. SMS capture has no metrics. Backup jobs have no success/failure tracking. When something breaks at 2am, there's no signal.

## Decision

**Observability is a delivery requirement, not a follow-up task.** When any infrastructure change ships, the corresponding observability changes ship with it or immediately after.

### What "observability" means for a new component:

| Component Type | Required Observability |
|---------------|----------------------|
| **New container** | Prometheus scrape target OR blackbox probe + health check + Grafana panel in Docker Containers dashboard |
| **New API endpoint** | Covered by existing express-prom-bundle (automatic). No extra work unless custom metrics needed. |
| **New external integration** | Custom metrics for call volume, error rate, latency (Counter + Histogram) |
| **New cron/batch job** | Metrics for: last run time, duration, success/failure count. Alert if job hasn't run in expected window. |
| **New collection/domain** | No extra observability unless it has its own service or cron job |

### Enforcement

- **Briefs**: When Silas briefs Kade on new infrastructure, the brief includes an "Observability" section.
- **Review**: Silas checks observability coverage as part of end-of-day review.
- **Dashboard audit**: Quarterly scan of all containers vs. scrape targets to catch drift.

### What this is NOT

- Not a mandate to instrument everything with custom metrics. The express-prom-bundle handles HTTP metrics automatically.
- Not a requirement for dashboards per feature. Most features are covered by the existing Service Overview and App Operations dashboards.
- Not an excuse to delay shipping. If observability takes more than 15-30 minutes, it can ship as an immediate follow-up (same day), not a blocker.

## Consequences

- New infrastructure briefs will include observability requirements
- Shared-observability configs (scrape targets, dashboards, alerts) stay current with infrastructure
- End-of-day review checklist includes observability drift check
- Minor overhead per feature (~15 min for blackbox probe + dashboard panel)

## Current Gaps (to be closed)

| Component | Gap | Fix |
|-----------|-----|-----|
| WebVOWL (8089) | No probe, no scrape | Add blackbox probe |
| SMS Capture | No metrics | Add custom counters in CaptureHandler |
| Pod Backup | No metrics | Add job metrics (textfile collector or push gateway) |
| Vikunja (3456) | No probe | Add blackbox probe |
| Fuseki | Probe only, no native metrics | Add Fuseki metrics scrape (if endpoint exists) or custom app metrics |
