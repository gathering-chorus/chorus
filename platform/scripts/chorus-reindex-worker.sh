#!/bin/bash
# chorus-reindex-worker.sh — run indexAllSources OUTSIDE the chorus-api process (#3085)
#
# Launched by the com.chorus.reindex-worker LaunchAgent on a 15-min interval —
# the same cadence the old in-process setInterval used. Single lock-guarded run;
# the LaunchAgent re-fires on the next interval.
#
# Unlike chorus-embed-worker.sh (which curls POST /api/chorus/embed because embed
# is async Ollama I/O the loop tolerates in-process), reindex is SYNCHRONOUS
# better-sqlite3 — it blocks whatever loop it runs on. So this runs the indexing
# directly in its OWN node process (dist/index-worker.js), writing SQLite directly;
# chorus-api's event loop is never touched (#3080 Track A / ADR-034).

set -euo pipefail

WORKER_JS="${CHORUS_REINDEX_WORKER_JS:-$HOME/CascadeProjects/chorus/platform/api/dist/index-worker.js}"
LOGFILE="${HOME}/.chorus/reindex-worker.log"
LOCKFILE="${HOME}/.chorus/reindex-worker.lock"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOGFILE"; }

# Single-run lock — preserves the old in-process "don't stomp an in-flight run"
# guarantee across process invocations. Recover a stale lock after 20 min
# (a pass should finish well within the 15-min cadence).
if [ -f "$LOCKFILE" ]; then
  lock_age=$(( $(date +%s) - $(stat -f %m "$LOCKFILE" 2>/dev/null || echo 0) ))
  if [ "$lock_age" -lt 1200 ]; then
    exit 0  # another run is active
  fi
  log "Stale lock (${lock_age}s), recovering"
  rm -f "$LOCKFILE"
fi
echo $$ > "$LOCKFILE"
trap 'rm -f "$LOCKFILE"' EXIT

if [ ! -f "$WORKER_JS" ]; then
  log "worker not built ($WORKER_JS) — skipping (deploy chorus-api to build dist/)"
  exit 0
fi

node "$WORKER_JS" >> "$LOGFILE" 2>&1
