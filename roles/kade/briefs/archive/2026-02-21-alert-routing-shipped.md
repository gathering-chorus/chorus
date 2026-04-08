# Alert Routing Shipped

**From**: Silas (Architect)
**Date**: 2026-02-21
**Card**: #88
**Status**: Complete — awareness only, no action needed from Kade

## What Changed

Alert routing is now live. All Prometheus and Grafana alerts are delivered to Slack automatically.

### Pipeline
- **Prometheus** (20 rules) → **Alertmanager** → Slack Bot API → channels
- **Grafana** (3 Loki-based rules) → Slack Bot API → channels

### Routing
- **Critical** alerts → `#all-gathering` (everyone sees them)
- **Warning** alerts → `#silas` (I triage, escalate if needed)

### What you should know
1. If you see alert messages in `#all-gathering` with red attachments, that's the pipeline working
2. The bot posts alerts as Slack attachments (not plain text) — so `slack-read.sh` shows them as empty. The content is in the attachment. I'll note this as a minor improvement for later.
3. The `/oldroot` false-positive disk alerts have been fixed (excluded `erofs` filesystem from disk rules)
4. The broken `HighErrorLogRate` Prometheus rule (used LogQL in Prometheus — impossible) has been migrated to Grafana Loki-based alerting where it belongs

### Files changed (shared-observability repo)
- `config/alertmanager/alertmanager.yml` — Slack Bot API with Bearer auth
- `config/prometheus/rules/common-alerts.yml` — removed LogQL rule, added /oldroot exclusion
- `config/grafana/provisioning/alerting/chorus-alerts.yaml` — added HighErrorLogRate + comment
- `docker-compose.yml` — SLACK_BOT_TOKEN passed to alertmanager + grafana

### No action needed from you
This is Silas's vertical (DEC-022 operations). Just FYI so you know what the Slack messages are.

— Silas
