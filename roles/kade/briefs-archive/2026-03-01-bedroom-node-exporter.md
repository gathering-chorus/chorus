# Brief: Wire node-exporter on Bedroom Mac

**From:** Silas
**Date:** 2026-03-01
**Priority:** P1
**Context:** Jeff wants PostHog local, but we have no unified infra governance across both machines. Before adding any new services, we need to see what we have.

## What

Install and run node-exporter on Bedroom Mac (192.168.86.242) so Prometheus on Library can scrape it. Then we get both machines' RAM/CPU/disk in one Grafana dashboard.

## Why

Right now Bedroom is a black box — 32GB RAM, ~17GB committed, 31 node processes, zero continuous monitoring. We're about to add PostHog (ClickHouse + Kafka + Postgres + Redis, ~4GB RAM). Jeff's direction: understand capacity before adding load. No buying software without knowing long-term infra cost.

## How

1. **Install node-exporter on Bedroom** — `brew install node_exporter` or download binary. Run as LaunchAgent (Silas will create the plist after you confirm it's running).
2. **Add scrape target to Prometheus** — in Library's `prometheus.yml`, add a job for `192.168.86.242:9100`.
3. **Build a "Data Center" Grafana dashboard** — two-panel view (Library + Bedroom) showing: RAM committed/free, CPU usage, disk usage. One place to see aggregate capacity.

## Acceptance Criteria

- node-exporter running on Bedroom, accessible at `192.168.86.242:9100/metrics`
- Prometheus scraping Bedroom successfully (check Targets page)
- Grafana dashboard showing both machines side-by-side: RAM, CPU, disk
- Jeff can open one dashboard and see "how healthy is my infrastructure?"

## Notes

- Bedroom has no Docker — this is a bare binary + LaunchAgent. Keep it simple.
- Don't install Docker on Bedroom for this — native binary only.
- After this ships, Silas will write a capacity budget doc and then plan PostHog placement.
