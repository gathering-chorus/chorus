#!/bin/bash
# chorus-embed-worker.sh — drain embed backlog in its OWN node process (#3379;
# was #1978's curl-the-API model).
#
# Runs as a LaunchAgent on a 5-minute interval. #1978 curled POST
# /api/chorus/embed, which ran the pass ON chorus-api's event loop — the pass
# interleaves synchronous better-sqlite3 page reads with lance writes, and on
# 2026-06-12 it wedged the API five times in one day (65-100% CPU, fs storms;
# convicted by isolation: 2.6% CPU calm with this worker disabled). Now the
# pass runs here, in dist/embed-delta-worker.js — the #3085 reindex-worker
# pattern: the API process is never touched.
#
# Each worker invocation processes one EMBED_PAGE_SIZE (100) page; this script
# loops with a pause until a pass embeds 0, then exits. The LaunchAgent
# restarts it on the next interval.

set -euo pipefail

WORKER_JS="${CHORUS_EMBED_WORKER_JS:-$HOME/CascadeProjects/chorus/platform/api/dist/embed-delta-worker.js}"
# Pin node by ABSOLUTE path (#3085/#3086): better-sqlite3 is a native module
# compiled for chorus-api's node; PATH-resolved node once found the wrong ABI
# and a worker died silently for days.
NODE_BIN="${CHORUS_NODE_BIN:-/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node}"
LOGFILE="${HOME}/.chorus/embed-worker.log"
LOCKFILE="${HOME}/.chorus/embed-worker.lock"
PAUSE_BETWEEN_BATCHES=2  # seconds — breathe room for Ollama
MAX_BATCHES=50           # safety cap per run — 50 * 100 = 5000 messages max

log() {
  echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOGFILE"
}

# Prevent concurrent runs — this lockfile is also the cross-process
# single-flight that #3214's in-process coalescing used to provide.
if [ -f "$LOCKFILE" ]; then
  lock_age=$(( $(date +%s) - $(stat -f %m "$LOCKFILE" 2>/dev/null || echo 0) ))
  if [ "$lock_age" -lt 600 ]; then
    exit 0  # another run is active (lock < 10 min old)
  fi
  log "Stale lock (${lock_age}s), recovering"
  rm -f "$LOCKFILE"
fi
echo $$ > "$LOCKFILE"
trap 'rm -f "$LOCKFILE"' EXIT

# Check Ollama is up (embed will fail without it)
OLLAMA_URL="${OLLAMA_URL:-http://127.0.0.1:11434}"
if ! curl -sf --max-time 3 "${OLLAMA_URL}/api/tags" > /dev/null 2>&1; then
  log "Ollama not reachable, skipping"
  exit 0
fi

if [ ! -f "$WORKER_JS" ]; then
  log "FAIL: worker js missing at $WORKER_JS — build chorus-api (dist not deployed?)"
  exit 1
fi

batch=0
total_embedded=0

while [ "$batch" -lt "$MAX_BATCHES" ]; do
  result=$("$NODE_BIN" "$WORKER_JS" 2>>"$LOGFILE" || echo '{"embedded":0,"error":"worker failed"}')

  embedded=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('embedded',0))" 2>/dev/null || echo 0)

  if [ "$embedded" -eq 0 ]; then
    break
  fi

  total_embedded=$((total_embedded + embedded))
  if [ $((total_embedded % 1000)) -lt 100 ]; then
    log "Progress: ${total_embedded} embedded so far"
  fi
  batch=$((batch + 1))

  sleep "$PAUSE_BETWEEN_BATCHES"
done

if [ "$total_embedded" -gt 0 ]; then
  log "Embedded ${total_embedded} messages in ${batch} batches"
fi
