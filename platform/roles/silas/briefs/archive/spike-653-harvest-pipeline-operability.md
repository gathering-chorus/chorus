# Spike: Harvest Pipeline Operability (#653)

**Question:** Should we build lightweight feedback loops around our existing manifests, or adopt a pipeline orchestrator?

**Answer:** Build. Four scripts, zero new containers.

## Current State

8 domain manifests, consistent schema (stages with `status`, `last_run`, counts). Harvest CLI (`scripts/harvest`) already does status, run, and auto-detect next stage. But none of this is wired into boot, alerting, board, or flow.

| Domain | Stages complete | Gaps |
|--------|----------------|------|
| blog | 4/4 | — |
| music | 4/5 (reconcile in-progress) | Source #2 unmounted, Source #3 17% imported |
| notes | 3/4 (verify pending) | 823 TTL unsynced |
| photos | 3/4 (verify pending) | 632 TTL unsynced, source count unknown |
| stories | partial | Manual pipeline, 48/87 TTL unloaded |
| sexuality | 2/4 | 9/29 volumes offline |
| facebook | 0/4 | Waiting for archive export |
| linkedin | 0/4 | Waiting for archive export |

## Why Not Adopt

| Tool | Problem |
|------|---------|
| **Dagster/Prefect/Airflow** | 2-4 GB RAM, 3+ containers, Python-native. Our stack is Node/TS. Overkill by 10x for 8 domains on 2 Macs. |
| **Temporal** | Lightest full orchestrator, TS SDK exists. Still needs Postgres/Cassandra. Wants to own execution — our harvesters would need rewriting. |
| **BullMQ** | Needs Redis. Designed for job queues, not multi-stage pipeline state. |
| **Agenda.js** | Needs MongoDB. Mongo is already a pain point. |

The core problem is **observability**, not execution. We already have harvesters. We already have manifests as the state store. We already have Prometheus + Grafana + alerting. The gap is a 100-line bridge between manifest JSON and the monitoring stack.

## Recommendation: Build 4 Integration Points

### 1. `harvest-exporter.sh` — Prometheus textfile exporter

Reads all 8 manifests, emits `.prom` file to `shared-observability/data/textfile_collector/harvest.prom`. Same pattern as `deploy_metrics.prom`, `fuseki_perf.prom`, etc.

Metrics:
```
harvest_stage_status{domain="music",stage="extract"} 2       # 0=not_started, 1=in_progress, 2=complete
harvest_stage_last_run_epoch{domain="music",stage="extract"} 1709123456
harvest_stage_record_count{domain="music",stage="extract"} 87386
harvest_domain_gap_count{domain="music"} 2
harvest_domain_updated_epoch{domain="music"} 1709123456
```

Run: LaunchAgent on 15-minute interval (no daemon).

### 2. `harvest-alerts.yml` — Prometheus alert rules

```yaml
- alert: HarvestStageStale
  expr: (time() - harvest_stage_last_run_epoch) > 172800    # 48h
  for: 1h
  labels: { domain: infra }

- alert: HarvestDomainDrift
  expr: harvest_domain_gap_count > 3
  for: 1h
  labels: { domain: infra }
```

### 3. Boot hook integration — `werk-init.sh` reads manifests

Add a harvest summary line to session-start context. One line per domain with staleness flag. Roles see pipeline health at boot without running `scripts/harvest` manually.

```
Harvest: music ● (reconcile 17%) | photos ◐ (verify pending, 48h stale) | notes ◐ | blog ● | ...
```

### 4. `harvest` CLI enhancement — `harvest sync-board`

When a domain goes stale (>48h since last stage run) or has >3 gaps, auto-update the scope card description with current manifest state. Keeps board cards honest without manual effort.

## Effort

| Script | Estimate |
|--------|----------|
| harvest-exporter.sh | 30 min (proven pattern) |
| harvest-alerts.yml | 15 min |
| Boot hook integration | 30 min |
| harvest sync-board | 45 min |
| Grafana harvest dashboard | 30 min |
| **Total** | ~2.5 hours |

## What This Gets Us

- Every role sees harvest health at session start
- Stale pipelines trigger alerts → cards → work
- Board cards stay current with reality
- Grafana dashboard shows pipeline state across all domains
- No new containers, no new languages, no new dependencies
- Existing harvest CLI gains one new subcommand

## What This Doesn't Do (and why that's fine)

- **Scheduling execution** — LaunchAgents already handle this. No need for a scheduler framework.
- **DAG visualization** — 8 linear pipelines don't need a DAG UI. The harvest CLI dashboard is sufficient.
- **Retry/backoff** — Harvesters are idempotent. Re-run manually or via LaunchAgent. No framework needed.
