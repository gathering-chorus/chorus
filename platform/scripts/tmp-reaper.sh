#!/usr/bin/env bash
# tmp-reaper.sh — Daily cleanup of orphaned /tmp team cruft
# Card #2057 | LaunchAgent: com.chorus.tmp-reaper
#
# Cleans: orphaned ZeroMQ sockets, stale alert/watchdog markers,
#         done pair files, posture/sentiment temp files
# Rotates: posture-timelapse/ and chorus-look/ (keep 7 days)
# Logs: structured JSON to stdout (Promtail picks up via LaunchAgent)

set -uo pipefail

KEEP_DAYS=7
DRY_RUN="${DRY_RUN:-0}"
CLEANED=0
ERRORS=0

log() {
  local level="$1" message="$2"
  local timestamp
  timestamp=$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S')
  echo "{\"timestamp\":\"$timestamp\",\"level\":\"$level\",\"appName\":\"tmp-reaper\",\"message\":\"$message\"}"
}

clean_file() {
  local path="$1" reason="$2"
  if [ "$DRY_RUN" = "1" ]; then
    log "info" "DRY RUN: would remove $path ($reason)"
  else
    rm -f "$path" 2>/dev/null && {
      ((CLEANED++))
      return 0
    } || {
      ((ERRORS++))
      log "warn" "Failed to remove $path"
      return 1
    }
  fi
}

clean_dir() {
  local path="$1" reason="$2"
  if [ "$DRY_RUN" = "1" ]; then
    log "info" "DRY RUN: would remove dir $path ($reason)"
  else
    rm -rf "$path" 2>/dev/null && {
      ((CLEANED++))
      return 0
    } || {
      ((ERRORS++))
      log "warn" "Failed to remove dir $path"
      return 1
    }
  fi
}

log "info" "Reaper starting"

# --- 1. Orphaned ZeroMQ IPC sockets ---
for sock in /tmp/zeb_def_ipc_*; do
  [ -e "$sock" ] || continue
  pid=$(echo "$sock" | grep -oE '[0-9]+' | tail -1)
  if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
    clean_file "$sock" "orphaned ZeroMQ socket, PID $pid dead"
  fi
done

# --- 2. Alert dedup markers older than 24h ---
find /tmp -maxdepth 1 -name "alert-*" -mtime +0 -print 2>/dev/null | while read -r f; do
  clean_file "$f" "stale alert marker >24h"
done

# --- 3. Watchdog markers older than 24h ---
find /tmp -maxdepth 1 -name "watchdog-*" -mtime +0 -print 2>/dev/null | while read -r f; do
  clean_file "$f" "stale watchdog marker >24h"
done

# --- 4. Pair files for Done/Won't Do cards ---
CARDS_CMD="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards"
for pf in /tmp/pair-*.md; do
  [ -e "$pf" ] || continue
  card_id=$(echo "$pf" | grep -oE '[0-9]+')
  if [ -n "$card_id" ]; then
    card_status=$("$CARDS_CMD" view "$card_id" 2>/dev/null | grep 'Status:' | awk '{print $2}' || echo "unknown")
    if [ "$card_status" = "Done" ] || [ "$card_status" = "Won't" ]; then
      clean_file "$pf" "pair file for $card_status card #$card_id"
    fi
  fi
done

# --- 5. Posture/sentiment temp files ---
for pattern in "posture-payload.*" "posture-response.*" "sentiment-payload.*" "sentiment-response.*"; do
  for f in /tmp/$pattern; do
    [ -e "$f" ] || continue
    clean_file "$f" "leaked posture/sentiment temp"
  done
done

# --- 6. Rotate posture-timelapse/ (keep KEEP_DAYS days) ---
if [ -d /tmp/posture-timelapse ]; then
  find /tmp/posture-timelapse -maxdepth 1 -type d -name "202*" -mtime +${KEEP_DAYS} -print 2>/dev/null | while read -r d; do
    clean_dir "$d" "posture-timelapse older than ${KEEP_DAYS} days"
  done
fi

# --- 7. Rotate chorus-look/ (keep KEEP_DAYS days) ---
if [ -d /tmp/chorus-look ]; then
  find /tmp/chorus-look -maxdepth 1 -type f -name "chrome-*" -mtime +${KEEP_DAYS} -print 2>/dev/null | while read -r f; do
    clean_file "$f" "chorus-look screenshot older than ${KEEP_DAYS} days"
  done
fi

# --- 8. claude-501/ — Claude Code tool result cache, safe to clean ---
if [ -d /tmp/claude-501 ]; then
  find /tmp/claude-501 -maxdepth 2 -type f -mtime +${KEEP_DAYS} -print 2>/dev/null | while read -r f; do
    clean_file "$f" "claude-501 cache older than ${KEEP_DAYS} days"
  done
fi

log "info" "Reaper complete: cleaned=$CLEANED errors=$ERRORS"
