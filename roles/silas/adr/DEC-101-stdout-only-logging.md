# DEC-101: Stdout-Only Logging

**Date:** 2026-04-13
**Status:** Active
**Owner:** Silas

## Context

Services write logs to /tmp/ via hardcoded file paths, bypassing Loki, Grafana, and alerts. 15 real 500 errors were found in /tmp/chorus-api.log that nobody saw. The monitoring system reported healthy while real failures hid in files Promtail doesn't watch.

## Decision

All services on both machines write to stdout/stderr only. No hardcoded log file paths.

**The rule:**
1. Apps write to stdout and stderr
2. LaunchAgent plists route stdout/stderr to `~/Library/Logs/Chorus/` or `~/Library/Logs/Gathering/`
3. Promtail watches those directories via glob scrape
4. Promtail ships to Loki
5. No service writes directly to /tmp/ or any other path

**Enforcement:**
- deep-health.sh checks for /tmp/*.log files and warns
- New plists must point StandardOutPath to ~/Library/Logs/

**Scope:** Machine-wide, both Library and Bedroom. Bedroom Promtail currently watches /tmp/ for some services — those migrate to ~/Library/Logs/ when plists are next touched.

## Consequences

- Every log line reaches Loki within seconds of being written
- Alerts can fire on any service error, not just the ones in watched directories
- No more shadow logs invisible to the team
- Services cannot silently fail — if it's not in Loki, it didn't happen
