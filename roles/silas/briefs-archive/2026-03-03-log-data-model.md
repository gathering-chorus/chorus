# Brief: Log Data Model — Gaps and Next Steps

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Card:** N/A — infrastructure improvement, multiple cards likely

## Context

Jeff and I mapped the full logging topology across Library Mac. Two artifacts now live in `data/about/`:
- `LOG_TOPOLOGY.html` — card-style inventory of every log destination, writers, readers, format, health status
- `LOG_RELATEDNESS.html` — OWL-style force-directed graph showing data flow relationships

We found **31 log destinations** across 5 layers: spine logs, board snapshots, team state files, LaunchAgent logs, and app/Docker logs.

## Key Findings

### Working Well
- **Spine → Loki pipeline is solid.** chorus.log, permission-prompts.log, command-errors.log, handoffs.log — all structured JSONL, collected by Promtail, queryable in Grafana. This is the gold standard.
- **Chorus index healthy.** 44,497 messages, 50MB, actively indexed.
- **Docker container logs collected** via docker_sd_configs — 6 app containers flowing to Loki.

### Gaps

1. **13 LaunchAgent /tmp logs go nowhere.** Not collected by Promtail. The two highest-volume logs in the system — `ops-agent.log` (2,597 lines, 272K) and `node-exporter.err` (9,557 lines, 2MB) — have zero readers. If these daemons fail, nobody knows until symptoms appear elsewhere.

2. **5 dead logs.** `andon-enrich.log` (0 lines), `jeff-input-monitor.log` (0 lines), `andon-light.log` (MISSING), `docker-startup.log` (MISSING), `harvest-exporter.err` (0 lines). Daemons may not be emitting, or may not be running.

3. **node-exporter.err is 2MB** — largest log file. All stderr, stdout is empty. Possible misconfiguration or very chatty stderr. Not collected anywhere.

4. **cost-log.md stale (6 days).** werk-init flags it but nobody acts.

## Proposed Next Steps (for your assessment)

1. **Add a Promtail scrape job for key /tmp daemon logs.** At minimum: ops-agent.log, defect-poller.log, fuseki-perf.log. These are the daemons that produce operational findings. Could bind-mount /tmp into the Promtail container or use a host path.

2. **Audit the 5 dead daemons.** Are andon-enrich, jeff-input-monitor, andon-light actually running? `launchctl list | grep com.chorus` would tell us. If they're running but not emitting, that's a different problem than not running.

3. **Triage node-exporter.err.** Is the 2MB of stderr real errors or informational noise? If noise, redirect to /dev/null. If real, it should flow to Loki.

4. **Standardize daemon log format.** The spine logs are structured JSONL — the daemon logs are unstructured text. If we're going to collect them, structured output would make them queryable.

5. **Consider log rotation for /tmp.** These files survive until reboot but grow unbounded. A logrotate config or size-capped approach would prevent surprises.

## Files

- `data/about/LOG_TOPOLOGY.html`
- `data/about/LOG_RELATEDNESS.html`

## Response Needed

Your architectural assessment of which gaps are worth closing and in what order. Some of these may already have cards or may not be worth the effort. You know the infra better than I do.
