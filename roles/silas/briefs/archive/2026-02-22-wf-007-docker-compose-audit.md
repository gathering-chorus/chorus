# Docker-Compose Audit: Shared Observability Stack

**Date**: 2026-02-22
**WF-007 Step 2** | Silas (Architect)

## Stack Overview

8 containers, all `restart: unless-stopped`, all on `observability-network`.
All ports bound to `127.0.0.1` (ADR-012 compliant).
All have healthchecks. Good baseline.

## Current Resource Usage (measured)

| Service | CPU | Memory | Data Volume |
|---------|-----|--------|-------------|
| Prometheus | 1.4% | 127 MB | 1.9 GB (prometheus-data) |
| Loki | 5.1% | 181 MB | 107 MB (loki-data) |
| Grafana | 6.5% | 177 MB | 2 MB (grafana-data) |
| Promtail | 1.3% | 91 MB | — (stateless) |
| Blackbox Exporter | 0.2% | 29 MB | — (stateless) |
| Alertmanager | 0.1% | 42 MB | — (no persistence!) |
| Node Exporter | 0.0% | 27 MB | — (stateless) |
| MySQL Exporter | 0.0% | 21 MB | — (stateless) |
| **Total** | **~15%** | **~695 MB** | **~2 GB** |

## Findings

### 1. No Resource Limits Set — FIXED

No container has `mem_limit` or `cpus` set. On a 16GB Mac Mini running 16+ containers, a single runaway service (Loki ingestion burst, Prometheus query) could OOM the host.

**Applied**: Conservative limits based on observed usage + 2x headroom.

### 2. Alertmanager Has No Persistence — FIXED

Alertmanager uses `/tmp/alertmanager.yml` from the `sed` entrypoint but has no named volume for silences/notification state. On restart, all silences and notification history are lost.

**Applied**: Added `alertmanager-data` named volume at `/alertmanager`.

### 3. Prometheus Data Volume Growing — MONITORED

1.9 GB with 15d retention. At current scrape rate (~38 targets), this is sustainable. Prometheus self-compacts. The `--storage.tsdb.retention.time=15d` flag is correct.

**No change needed** — existing disk alerts (C1 constraint) cover this.

### 4. Loki Retention is 7 Days — VERIFIED

`retention_period: 168h` in loki-config.yaml. Compactor runs every 10m. 107 MB current volume. App is logging 69K entries/30min (Kade's Step 1 finding) — Loki handles this fine at current limits (`ingestion_rate_mb: 16`).

### 5. Promtail Position Tracking — FIXED

Promtail stores file read positions in `/tmp/positions.yaml` by default. On container restart, it re-reads all Docker logs from the beginning, causing duplicate log entries in Loki. Should use a named volume for position persistence.

**Applied**: Added `promtail-positions` named volume.

### 6. Image Tags — NOTED

Most images use pinned versions (good). Alertmanager uses `latest` (bad — unpredictable updates on pull).

**Applied**: Pinned alertmanager to `v0.26.0`.

## Failure Mode Analysis

| Failure | Impact | Recovery |
|---------|--------|----------|
| Prometheus down | No metrics scraping, alerts stop firing | Auto-restart. Gap in metrics (acceptable for 15d window) |
| Loki down | Log ingestion stops. Promtail buffers briefly | Auto-restart. Promtail resumes from position file |
| Grafana down | Dashboards unavailable | Auto-restart. No data loss (visualization only) |
| Promtail down | Logs stop flowing to Loki | Auto-restart. Position file ensures no duplicate/missing logs |
| Alertmanager down | Alerts fire but don't route to Slack | Auto-restart. Prometheus buffers alerts briefly |
| Blackbox down | Probe-based alerts stop | Auto-restart. No cascading impact |
| Full stack down | Complete observability loss | `docker compose up -d`. All data persisted in named volumes |
| Host OOM | Docker kills highest-memory container first | Resource limits prevent cascade. Loki/Grafana capped first |
