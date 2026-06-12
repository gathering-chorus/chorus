#!/bin/bash
# daily-review-ops.sh — 6am ops health check, posts to Bridge
# Card #1766 | DEC-107 compliant (no osascript)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_LOG="$SCRIPT_DIR/chorus-log"
TIMESTAMP=$(TZ=America/New_York date '+%Y-%m-%d %H:%M')
STATUS="green"
ISSUES=""

# --- LaunchAgent health ---
AGENTS_EXPECTED=(
  com.gathering.app
  com.gathering.fuseki
  com.chorus.api
  com.chorus.context-cache-daily
  com.chorus.launchagent-metrics
)
AGENTS_DOWN=""
for agent in "${AGENTS_EXPECTED[@]}"; do
  if ! launchctl list "$agent" &>/dev/null; then
    AGENTS_DOWN+="  - $agent\n"
  fi
done
if [ -n "$AGENTS_DOWN" ]; then
  STATUS="red"
  ISSUES+="**LaunchAgents down:**\n$AGENTS_DOWN\n"
fi

# --- Service health ---
SERVICES=(
  "App|http://localhost:3000/health"
  "Chorus API|http://localhost:3340/health"
  "Messaging|http://localhost:3475/health"
  "Fuseki|http://localhost:3030/$/ping"
  "Grafana|http://localhost:3100/api/health"
)
SERVICES_DOWN=""
for svc in "${SERVICES[@]}"; do
  NAME="${svc%%|*}"
  URL="${svc##*|}"
  if ! curl -sf --max-time 3 "$URL" &>/dev/null; then
    SERVICES_DOWN+="  - $NAME ($URL)\n"
  fi
done
if [ -n "$SERVICES_DOWN" ]; then
  STATUS="red"
  ISSUES+="**Services unreachable:**\n$SERVICES_DOWN\n"
fi

# --- Disk (APFS-aware, no df — see feedback_apfs_reporting.md) ---
DISK_PCT=$(python3 -c "
import subprocess, re
out = subprocess.check_output(['diskutil', 'info', '/'], text=True)
total = int(re.search(r'Container Total Space:.*\((\d+) Bytes\)', out).group(1))
free = int(re.search(r'Container Free Space:.*\((\d+) Bytes\)', out).group(1))
print(int(((total - free) / total) * 100))
" 2>/dev/null || echo "0")
if [ "${DISK_PCT:-0}" -gt 90 ]; then
  STATUS="red"
  ISSUES+="**Disk:** ${DISK_PCT}% used (>90% critical)\n"
elif [ "${DISK_PCT:-0}" -gt 85 ]; then
  [ "$STATUS" = "green" ] && STATUS="yellow"
  ISSUES+="**Disk:** ${DISK_PCT}% used (>85% threshold)\n"
fi

# --- Alert state (check if alerts fired overnight) ---
ALERT_STATE_DIR="$HOME/Library/Logs/Gathering/alert-state"
ALERT_COOLDOWN_DIR="/tmp"
ACTIVE_ALERTS=""
# Check alert-runner cooldown files (nudge-stale, etc.)
for f in "$ALERT_COOLDOWN_DIR"/alert-nudge-*; do
  [ -f "$f" ] || continue
  if find "$f" -mmin -1440 -print -quit 2>/dev/null | grep -q .; then
    component=$(basename "$f" | sed 's/^alert-nudge-//;s/-[0-9].*$//')
    # Deduplicate — only report each component once
    echo "$ACTIVE_ALERTS" | grep -q "$component" && continue
    ACTIVE_ALERTS+="  - $component (alert-runner)\n"
  fi
done
# Check infra-alert daily fire state
for f in "$ALERT_STATE_DIR"/*-"$(date +%Y-%m-%d)".fired; do
  [ -f "$f" ] || continue
  component=$(basename "$f" | sed "s/-$(date +%Y-%m-%d).fired$//")
  ACTIVE_ALERTS+="  - $component (infra-alert)\n"
done
# Check strike counts > 0
for f in "$ALERT_STATE_DIR"/*.strikes; do
  [ -f "$f" ] || continue
  strikes=$(cat "$f" 2>/dev/null || echo "0")
  if [ "$strikes" -gt 0 ] 2>/dev/null; then
    component=$(basename "$f" .strikes)
    ACTIVE_ALERTS+="  - $component ($strikes consecutive failures)\n"
  fi
done
if [ -n "$ACTIVE_ALERTS" ]; then
  STATUS="red"
  ISSUES+="**Alerts fired overnight:**\n$ACTIVE_ALERTS\n"
fi

# --- Build summary ---
if [ "$STATUS" = "green" ]; then
  BODY="🟢 **Ops Review** — $TIMESTAMP\n\nAll systems healthy. Disk: ${DISK_PCT:-?}%."
elif [ "$STATUS" = "yellow" ]; then
  BODY="🟡 **Ops Review** — $TIMESTAMP\n\n$ISSUES"
else
  BODY="🔴 **Ops Review** — $TIMESTAMP\n\n$ISSUES"
fi

# --- Emit health check event ---
HEALTH_STATUS=$( [ "$STATUS" = "green" ] && echo "pass" || echo "fail" )
"$CHORUS_LOG" ops.health.checked silas status=$HEALTH_STATUS disk=${DISK_PCT:-0} >/dev/null 2>&1 || true

# --- Emit completion event (no Bridge post — summary script handles that) ---
"$CHORUS_LOG" ops.review.completed silas status=$STATUS >/dev/null 2>&1 || true


# --- Nudge Silas on ops failures ---
OPS_NUDGE="$SCRIPT_DIR/ops-nudge"
if [ "$STATUS" = "red" ]; then
  "$OPS_NUDGE" silas "[ops] $TIMESTAMP — $( echo -e "$ISSUES" | head -3 )" >/dev/null 2>&1 || true
fi

echo -e "$BODY"
