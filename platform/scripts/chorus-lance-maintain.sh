#!/bin/bash
# chorus-lance-maintain.sh — nightly lance compact+prune+reindex OFF-PROCESS (#3379)
#
# Launched by com.chorus.lance-maintain (StartCalendarInterval 03:30, after the
# 03:00 nightly suites). Runs dist/lance-maintain-worker.js in its OWN node
# process — the #3085 reindex-worker pattern: heavy fs/lance work never touches
# chorus-api's event loop (the 2026-06-12 wedge class: 35,032 fragments,
# fs.AfterStat storms, 3 outages in one day).
#
# Loud on miss: logs to ~/Library/Logs/Gathering/lance-maintain.log, which
# deep-health's DAILY_LOGS freshness check watches (25h threshold) — a skipped
# night surfaces in the 6am ops review without any new alert plumbing.

set -euo pipefail

WORKER_JS="${CHORUS_LANCE_MAINTAIN_JS:-$HOME/CascadeProjects/chorus/platform/api/dist/lance-maintain-worker.js}"
# Pin node by ABSOLUTE path (#3085/#3086 lesson): PATH-resolved node once found a
# different ABI and a native-module worker died silently for days.
NODE_BIN="${CHORUS_NODE_BIN:-/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node}"
LOGFILE="${HOME}/Library/Logs/Gathering/lance-maintain.log"
LOCKFILE="${HOME}/.chorus/lance-maintain.lock"
CHORUS_LOG="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/platform/scripts/chorus-log"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOGFILE"; }

# Single-run lock with stale-lock recovery (a pass finishes in minutes; 30 min
# means something is wrong — recover and let this run proceed).
if [ -f "$LOCKFILE" ]; then
  lock_age=$(( $(date +%s) - $(stat -f %m "$LOCKFILE" 2>/dev/null || echo 0) ))
  if [ "$lock_age" -lt 1800 ]; then
    log "skip: another run is active (lock ${lock_age}s old)"
    exit 0
  fi
  log "stale lock (${lock_age}s), recovering"
  rm -f "$LOCKFILE"
fi
echo $$ > "$LOCKFILE"
trap 'rm -f "$LOCKFILE"' EXIT

log "start"
if result=$("$NODE_BIN" "$WORKER_JS" 2>>"$LOGFILE"); then
  log "ok: $result"
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "lance.maintain.completed" "silas" "$result" >/dev/null 2>&1 || true
else
  rc=$?
  log "FAIL: worker exit $rc"
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "lance.maintain.failed" "silas" "exit=$rc" >/dev/null 2>&1 || true
  exit "$rc"
fi
