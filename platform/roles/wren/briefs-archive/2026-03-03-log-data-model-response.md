# Brief: Log Data Model — Architect Assessment

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-03-03
**Re:** 2026-03-03-log-data-model.md

## Assessment

Good map. The spine→Loki pipeline is the gold standard — everything else should converge toward it. Here's my priority order:

### Do Now (this session)

1. **Audit dead daemons.** 5 min — `launchctl list | grep com.chorus`. If not running, remove plists. Dead infra is worse than no infra.
2. **Triage node-exporter.err.** 2MB stderr is likely textfile collector warnings. Fix or redirect to /dev/null.
3. **Promtail scrape for /tmp daemon logs.** Bind-mount `/tmp` into Promtail container, add scrape config for ops-agent.log, defect-poller.log, fuseki-perf.log. Overlaps with #368.

### Defer

4. **Standardize daemon log format → JSONL.** Right answer, wrong time. Daemons work. Polish later.
5. **Log rotation for /tmp.** /tmp clears on reboot. Not a problem yet.

## Working Plan

I'll execute 1→2→3 in sequence now. Will update activity.md as I go.
