#!/bin/bash
# Infrastructure Alerting (#1654)
# Runs every 5 minutes via LaunchAgent. Checks critical infrastructure.
# 3-strike rule: only alert after 3 consecutive failures.
# Delivers to The Clearing API so Jeff sees it on Bridge.

set -euo pipefail

NUDGE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge"
STATE_DIR="$HOME/Library/Logs/Gathering/alert-state"
mkdir -p "$STATE_DIR"

alert() {
  local level=$1 component=$2 message=$3
  local icon="⚠"
  [ "$level" = "critical" ] && icon="🔴"

  # Daily suppression — one alert per component per day
  local fired_file="$STATE_DIR/${component}-$(date +%Y-%m-%d).fired"
  if [ -f "$fired_file" ]; then
    echo "[$(date '+%H:%M:%S')] $level: $component — suppressed (already fired today)"
    return 0
  fi
  touch "$fired_file"

  # Nudge owning role directly — not the Bridge
  "$NUDGE" silas "${icon} ALERT: ${component} — ${message}" --force 2>/dev/null || true

  echo "[$(date '+%H:%M:%S')] $level: $component — $message"
}

check_strikes() {
  local component=$1
  local file="$STATE_DIR/${component}.strikes"
  local current=$(cat "$file" 2>/dev/null || echo 0)
  echo $((current + 1)) > "$file"
  echo $((current + 1))
}

clear_strikes() {
  local component=$1
  echo 0 > "$STATE_DIR/${component}.strikes"
  rm -f "$STATE_DIR/${component}-$(date +%Y-%m-%d).fired"
}

MAX_STRIKES=3

# === Check 1: Bedroom SSH ===
if ssh -o ConnectTimeout=5 -o BatchMode=yes jeffbridwell@192.168.86.242 "echo ok" > /dev/null 2>&1; then
  clear_strikes "bedroom-ssh"
else
  STRIKES=$(check_strikes "bedroom-ssh")
  if [ "$STRIKES" -ge "$MAX_STRIKES" ]; then
    alert "critical" "Bedroom Mac" "SSH unreachable for $STRIKES consecutive checks"
  fi
fi

# === Check 2: Library disk ===
DISK_PCT=$(df -h / | tail -1 | awk '{print $5}' | tr -d '%')
if [ "$DISK_PCT" -ge 95 ]; then
  alert "critical" "Library Disk" "${DISK_PCT}% used — CRITICAL"
elif [ "$DISK_PCT" -ge 90 ]; then
  alert "warning" "Library Disk" "${DISK_PCT}% used — WARNING"
else
  clear_strikes "disk"
fi

# === Check 3: Fuseki health ===
FUSEKI_STATUS=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 'http://localhost:3030/$/ping' 2>/dev/null || echo "000")
if [ "$FUSEKI_STATUS" = "200" ]; then
  clear_strikes "fuseki"
else
  STRIKES=$(check_strikes "fuseki")
  if [ "$STRIKES" -ge "$MAX_STRIKES" ]; then
    alert "critical" "Fuseki" "Health check failed (HTTP $FUSEKI_STATUS) for $STRIKES checks"
  fi
fi

# === Check 4: NiFi queue backup ===
NIFI_TOKEN_FILE="$HOME/Library/Logs/Gathering/nifi-token"
if [ -f "$NIFI_TOKEN_FILE" ]; then
  NIFI_QUEUED=$(ssh -o ConnectTimeout=5 jeffbridwell@192.168.86.242 "curl -sk 'https://jeffs-mac-mini.lan:8443/nifi-api/flow/process-groups/root' -H 'Authorization: Bearer $(cat $NIFI_TOKEN_FILE)' 2>/dev/null" 2>/dev/null | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    total = sum(pg.get('status',{}).get('aggregateSnapshot',{}).get('flowFilesQueued',0) for pg in d.get('processGroupFlow',{}).get('flow',{}).get('processGroups',[]))
    print(total)
except:
    print(-1)
" 2>/dev/null || echo -1)

  if [ "$NIFI_QUEUED" -gt 1000 ] 2>/dev/null; then
    alert "warning" "NiFi" "$NIFI_QUEUED FlowFiles queued — possible backup"
  elif [ "$NIFI_QUEUED" -ge 0 ] 2>/dev/null; then
    clear_strikes "nifi-queue"
  fi
fi

# === Check 5: The Clearing alive ===
CLEARING_STATUS=$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 'http://localhost:3470/health' 2>/dev/null || echo "000")
if [ "$CLEARING_STATUS" = "200" ]; then
  clear_strikes "clearing"
else
  STRIKES=$(check_strikes "clearing")
  if [ "$STRIKES" -ge "$MAX_STRIKES" ]; then
    alert "warning" "The Clearing" "Health check failed (HTTP $CLEARING_STATUS) for $STRIKES checks"
  fi
fi

# === Check 6: App alive ===
APP_STATUS=$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 'http://localhost:3000/health' 2>/dev/null || echo "000")
if [ "$APP_STATUS" = "200" ]; then
  clear_strikes "app"
else
  STRIKES=$(check_strikes "app")
  if [ "$STRIKES" -ge "$MAX_STRIKES" ]; then
    alert "critical" "Gathering App" "Health check failed (HTTP $APP_STATUS) for $STRIKES checks"
  fi
fi

# === Check 7: Cloudflare tunnel (end-to-end) ===
# Internal health passes but external requests fail = stale tunnel.
# This is the check that would have caught the 2026-03-28 seed outage.
TUNNEL_STATUS=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 'https://lightlifeurbangardens.com/health' 2>/dev/null || echo "000")
if [ "$TUNNEL_STATUS" = "200" ]; then
  clear_strikes "cloudflare-tunnel"
else
  STRIKES=$(check_strikes "cloudflare-tunnel")
  if [ "$STRIKES" -ge "$MAX_STRIKES" ]; then
    alert "critical" "Cloudflare Tunnel" "External health check failed (HTTP $TUNNEL_STATUS) for $STRIKES checks — inbound webhooks (Twilio seeds) are dead. Restart: launchctl kickstart -k gui/\$(id -u)/com.cloudflare.tunnel"
  fi
fi
