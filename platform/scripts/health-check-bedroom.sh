#!/usr/bin/env bash
# health-check-bedroom.sh — Automated Bedroom Mac health check
# Runs on Library, SSHs to Bedroom, checks services + metrics.
# Designed for cron/LaunchAgent — outputs one-line status or cards failures.
#
# Usage: health-check-bedroom.sh [--card]
#   --card: create board cards for failures (default: just report)

set -eo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
BEDROOM="Jeffs-Mac-mini.local"
BOARD_TS="$CHORUS_ROOT/platform/scripts/cards"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"
CARD_MODE=false
[ "${1:-}" = "--card" ] && CARD_MODE=true

FAILURES=()
WARNINGS=()

# 1. SSH reachability
if ! ssh -o ConnectTimeout=5 -o BatchMode=yes "$BEDROOM" "echo ok" >/dev/null 2>&1; then
  FAILURES+=("Bedroom SSH unreachable")
  echo "CRITICAL: Bedroom SSH unreachable"
  if $CARD_MODE; then
    bash "$BOARD_TS" add "SWAT: Bedroom Mac unreachable via SSH" --owner silas --priority P1 -q 2>/dev/null || true
  fi
  bash "$CHORUS_LOG" health.bedroom.failed silas detail="SSH unreachable" 2>/dev/null || true
  exit 1
fi

# 2. Node exporter (Prometheus scrape target)
NODE_EXPORTER=$(ssh -o ConnectTimeout=5 "$BEDROOM" "curl -s -o /dev/null -w '%{http_code}' http://localhost:9100/metrics 2>/dev/null || echo 000" 2>/dev/null)
if [ "$NODE_EXPORTER" != "200" ]; then
  FAILURES+=("Node exporter down (HTTP $NODE_EXPORTER)")
fi

# 3. Key services on Bedroom
for svc in "com.gathering.images-api" "com.gathering.ollama" "com.gathering.video-server" "com.gathering.node-exporter"; do
  STATUS=$(ssh -o ConnectTimeout=5 "$BEDROOM" "launchctl list '$svc' 2>/dev/null | grep -c 'PID' || echo 0" 2>/dev/null)
  if [ "$STATUS" = "0" ]; then
    WARNINGS+=("$svc not running")
  fi
done

# 3b. Ollama model health — verify embedding endpoint actually works (#1855)
#
# Timeout raised 10s → 45s on 2026-08-09, after the SIXTH false alarm.
#
# Measured at the moment of tonight's 03:42 alarm: /api/tags answered in 4ms and
# a real embedding returned 768 dimensions in 9.68s. Working, and 0.3s from being
# called broken. The probe was not detecting an outage — it was detecting that
# the model had been idle long enough to need loading, which is the normal state
# of that machine at three in the morning.
#
# Five earlier fires were each read, recognised as known noise, and left armed.
# That is the actual danger: a monitor everyone has learned to dismiss will be
# dismissed on the night it is right.
#
# 45s deliberately sits well clear of cold-load rather than just above the
# observed number — a ceiling tuned to observed-plus-a-bit reproduces this bug
# the first time the machine is busier than the day it was measured. Slowness is
# now a WARNING carrying the measured seconds, so drift toward genuinely broken
# is visible while it is still only drift.
OLLAMA_T0=$(date +%s)
OLLAMA_DIM=$(ssh -o ConnectTimeout=5 "$BEDROOM" "curl -sf --max-time 45 http://localhost:11434/api/embeddings -d '{\"model\":\"nomic-embed-text\",\"prompt\":\"health check\"}' 2>/dev/null | python3 -c 'import sys,json; print(len(json.load(sys.stdin).get(\"embedding\",[])))' 2>/dev/null || echo 0" 2>/dev/null)
OLLAMA_SECS=$(( $(date +%s) - OLLAMA_T0 ))
if [ "$OLLAMA_DIM" = "0" ] || [ "$OLLAMA_DIM" = "" ]; then
  FAILURES+=("Ollama embedding failed after ${OLLAMA_SECS}s — model unloaded or broken")
elif [ "$OLLAMA_DIM" != "768" ]; then
  WARNINGS+=("Ollama embedding returned unexpected dim=$OLLAMA_DIM (expected 768)")
elif [ "$OLLAMA_SECS" -gt 20 ]; then
  WARNINGS+=("Ollama embedding OK but slow: ${OLLAMA_SECS}s (cold load; failure threshold 45s)")
fi

# 3c. Fuseki — not installed on Bedroom. Library (Jeffs-Mac-Mini-M1-3.local:3030) is primary.

# 3d. NiFi process check (#1853 feedback from Kade)
NIFI_PID=$(ssh -o ConnectTimeout=5 "$BEDROOM" "pgrep -f 'nifi.*run' 2>/dev/null || echo 0" 2>/dev/null)
if [ "$NIFI_PID" = "0" ] || [ -z "$NIFI_PID" ]; then
  WARNINGS+=("NiFi process not running on Bedroom")
fi

# 4. Disk space on Bedroom external drives
DISK_INFO=$(ssh -o ConnectTimeout=5 "$BEDROOM" "df -h /Volumes/VideosNew 2>/dev/null | tail -1 | awk '{print \$5}'" 2>/dev/null || echo "unknown")
DISK_PCT="${DISK_INFO%%%}"
if [ "$DISK_PCT" != "unknown" ] && [ "$DISK_PCT" -gt 90 ] 2>/dev/null; then
  WARNINGS+=("Bedroom external disk at ${DISK_PCT}%")
fi

# 5. Memory pressure
MEM_PRESSURE=$(ssh -o ConnectTimeout=5 "$BEDROOM" "memory_pressure 2>/dev/null | grep 'System-wide memory free percentage' | awk '{print \$NF}' | tr -d '%'" 2>/dev/null || echo "unknown")
if [ "$MEM_PRESSURE" != "unknown" ] && [ "$MEM_PRESSURE" -lt 10 ] 2>/dev/null; then
  WARNINGS+=("Bedroom memory pressure: ${MEM_PRESSURE}% free")
fi

# Report
TOTAL_ISSUES=$(( ${#FAILURES[@]} + ${#WARNINGS[@]} ))

if [ $TOTAL_ISSUES -eq 0 ]; then
  echo "OK: Bedroom healthy (SSH, node_exporter, services, disk)"
  bash "$CHORUS_LOG" health.bedroom.ok silas 2>/dev/null || true
else
  for f in "${FAILURES[@]}"; do
    echo "FAIL: $f"
    if $CARD_MODE; then
      bash "$BOARD_TS" add "SWAT: $f" --owner silas --priority P1 -q 2>/dev/null || true
    fi
  done
  for w in "${WARNINGS[@]}"; do
    echo "WARN: $w"
  done
  bash "$CHORUS_LOG" health.bedroom.issues silas failures="${#FAILURES[@]}" warnings="${#WARNINGS[@]}" 2>/dev/null || true
  # Nudge silas on any issue — cooldown 1hr
  COOLDOWN="/tmp/bedroom-health-$(date '+%Y-%m-%d-%H')"
  if [ ! -f "$COOLDOWN" ]; then
    touch "$COOLDOWN"
    ALL_ISSUES=""
    for f in "${FAILURES[@]}"; do ALL_ISSUES="$ALL_ISSUES FAIL:$f"; done
    for w in "${WARNINGS[@]}"; do ALL_ISSUES="$ALL_ISSUES WARN:$w"; done
    # #2808: bash `nudge` retired in #2804/#2809. Use ops-nudge (pulse-direct).
    OPS_NUDGE="$CHORUS_ROOT/platform/scripts/ops-nudge"
    "$OPS_NUDGE" silas "bedroom-health:$ALL_ISSUES" 2>/dev/null || true
  fi
fi
