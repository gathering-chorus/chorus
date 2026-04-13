#!/bin/bash
# chorus-embed-worker.sh — drain embed backlog via API, outside the API process (#1978)
# Runs as a LaunchAgent on a 5-minute interval. Processes batches with a pause
# between each to avoid hammering Ollama and starving other consumers.
#
# The API's POST /api/chorus/embed processes EMBED_PAGE_SIZE (100) messages per call.
# This worker calls it repeatedly with a 2s pause until the batch returns 0,
# then exits. The LaunchAgent restarts it on the next interval.

set -euo pipefail

API="${CHORUS_API:-http://localhost:3340}"
LOGFILE="${HOME}/.chorus/embed-worker.log"
LOCKFILE="${HOME}/.chorus/embed-worker.lock"
PAUSE_BETWEEN_BATCHES=2  # seconds — breathe room for Ollama + API
MAX_BATCHES=50           # safety cap per run — 50 * 100 = 5000 messages max

log() {
  echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOGFILE"
}

# Prevent concurrent runs
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

# Check API is up
if ! curl -sf --max-time 3 "${API}/api/chorus/health" > /dev/null 2>&1; then
  log "API not reachable, skipping"
  exit 0
fi

# Check Ollama is up (embed will fail without it)
OLLAMA_URL="${OLLAMA_URL:-http://127.0.0.1:11434}"
if ! curl -sf --max-time 3 "${OLLAMA_URL}/api/tags" > /dev/null 2>&1; then
  log "Ollama not reachable, skipping"
  exit 0
fi

batch=0
total_embedded=0

while true; do
  result=$(curl -sf --max-time 120 -X POST "${API}/api/chorus/embed" 2>/dev/null || echo '{"embedded":0,"error":"request failed"}')

  embedded=$(echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin).get('embedded',0))" 2>/dev/null || echo 0)

  if [ "$embedded" -eq 0 ]; then
    break
  fi

  total_embedded=$((total_embedded + embedded))
  if [ $((total_embedded % 1000)) -lt 100 ]; then
    remaining=$(curl -sf --max-time 5 "${API}/api/chorus/health" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('unembedded',0))" 2>/dev/null || echo "?")
    log "Progress: ${total_embedded} embedded so far (${remaining} remaining)"
  fi
  batch=$((batch + 1))

  sleep "$PAUSE_BETWEEN_BATCHES"
done

if [ "$total_embedded" -gt 0 ]; then
  # Get remaining count
  remaining=$(curl -sf --max-time 5 "${API}/api/chorus/health" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('unembedded',0))" 2>/dev/null || echo "?")
  log "Embedded ${total_embedded} messages in ${batch} batches (${remaining} remaining)"
fi
