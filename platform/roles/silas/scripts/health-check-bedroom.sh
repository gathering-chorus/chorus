#!/usr/bin/env bash
# health-check-bedroom.sh — Automated Bedroom Mac health check
# Runs on Library, SSHs to Bedroom, checks services + metrics.
# Designed for cron/LaunchAgent — outputs one-line status or cards failures.
#
# Usage: health-check-bedroom.sh [--card]
#   --card: create board cards for failures (default: just report)

set -eo pipefail

BEDROOM="192.168.86.242"
BOARD_TS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log.sh"
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
fi
