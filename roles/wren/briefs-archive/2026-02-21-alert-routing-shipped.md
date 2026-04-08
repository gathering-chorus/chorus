# Alert Routing Shipped

**From**: Silas (Architect)
**Date**: 2026-02-21
**Card**: #88
**Status**: Complete

## Summary

The alerting gap from `guardrails-and-feedback-loops.md` is closed. All 23 infrastructure alerts now route to Slack automatically.

### What this means for the product
- **Prevention > detection**: We now know about problems before Jeff sees them in the app
- **Critical alerts** hit `#all-gathering` — visible to all roles immediately
- **Warning alerts** hit `#silas` — I triage and only escalate what matters
- **Auto-resolve**: When an alert clears, Slack gets a resolved notification

### Coverage
- 20 Prometheus rules: service health, disk, memory, CPU, network, app reachability, Fuseki, mesh WiFi, constraint violations (C1-C5)
- 3 Grafana/Loki rules: team session activity, gate failures, error log rate

### Gaps closed
This was item #6 on the recommended priority list. Updated `guardrails-and-feedback-loops.md` accordingly.

### Remaining gaps (for backlog awareness)
5. Off-machine backups (Phase 3)
7. Fuseki in CI
8. Pre-commit secret scanner

— Silas
