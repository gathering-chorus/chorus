#!/bin/bash
# watchdog.sh — Team awareness stall detection (#1958)
# Reads role-state timestamps, nudges stale roles, escalates to Wren then Jeff.
#
# Thresholds: 2min → nudge role, 3min → escalate to Wren, 5min → alert Jeff
# Runs every 60s via LaunchAgent com.chorus.watchdog
#
# State tracking: /tmp/watchdog-{role}.state (last nudge time + escalation level)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OPS_NUDGE="$SCRIPT_DIR/ops-nudge"
CHORUS_LOG="$SCRIPT_DIR/../logs/chorus.log"
SCAN_DIR="/tmp/claude-team-scan"
WATCHDOG_DIR="/tmp/watchdog"
BRIDGE="http://localhost:3470/api/message"

# #2053: raised from 5/10/15min — demo→accept gap is 5-10min, 5min nudge is always noise
NUDGE_THRESHOLD=600    # 10 minutes
ESCALATE_THRESHOLD=900 # 15 minutes
ALERT_THRESHOLD=1200   # 20 minutes

mkdir -p "$WATCHDOG_DIR"

now=$(date +%s)
all_inactive=true
inactive_count=0

for role in wren silas kade; do
  STATE_FILE="$SCAN_DIR/${role}-declared.json"
  WATCHDOG_STATE="$WATCHDOG_DIR/${role}.state"

  # Skip if no state file
  if [ ! -f "$STATE_FILE" ]; then
    continue
  fi

  # Read state timestamp and current state
  STATE_TS=$(python3 -c "import json; d=json.load(open('$STATE_FILE')); print(d.get('ts',0))" 2>/dev/null || echo "0")
  STATE_VAL=$(python3 -c "import json; d=json.load(open('$STATE_FILE')); print(d.get('state','unknown'))" 2>/dev/null || echo "unknown")
  CARD=$(python3 -c "import json; d=json.load(open('$STATE_FILE')); print(d.get('card',''))" 2>/dev/null || echo "")

  # Skip idle/waiting/observing roles — they're not expected to be active (#1891)
  if [ "$STATE_VAL" = "idle" ] || [ "$STATE_VAL" = "waiting" ] || [ "$STATE_VAL" = "observing" ]; then
    continue
  fi

  age=$((now - STATE_TS))

  # AC#2: Check if the card is Done/accepted — no alerts on finished work (#2033)
  if [ -n "$CARD" ]; then
    CARD_STATUS=$(bash "$SCRIPT_DIR/cards" view "$CARD" 2>/dev/null | grep -oE 'Status:\s+\S+' | awk '{print $2}' || echo "unknown")
    if [ "$CARD_STATUS" = "Done" ] || [ "$CARD_STATUS" = "Won't" ]; then
      # Card is done — role hasn't updated state yet. Skip, don't alert.
      continue
    fi
  fi

  # AC#3: State gap tolerance — if state is very recent (< 30s), skip.
  # Covers the gap between /acp completing and next /pull declaring new state.
  if [ "$age" -lt 30 ]; then
    all_inactive=false
    continue
  fi

  # Check observation file for recent tool calls
  OBS_FILE="$SCAN_DIR/${role}-observations.jsonl"
  last_obs=0
  if [ -f "$OBS_FILE" ]; then
    last_obs=$(tail -1 "$OBS_FILE" 2>/dev/null | python3 -c "
import sys,json
from datetime import datetime,timezone
try:
  d=json.load(sys.stdin)
  ts=d.get('ts','')
  if ts.endswith('Z'):
    dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)
  else:
    dt=datetime.fromisoformat(ts)
  import time
  print(int(dt.timestamp()))
except: print(0)" 2>/dev/null || echo "0")
  fi

  obs_age=$((now - last_obs))

  # Use the more recent of state change or observation
  effective_age=$age
  if [ "$last_obs" -gt "$STATE_TS" ]; then
    effective_age=$obs_age
  fi

  # Read watchdog state (last action)
  last_action="none"
  last_action_ts=0
  if [ -f "$WATCHDOG_STATE" ]; then
    last_action=$(head -1 "$WATCHDOG_STATE" 2>/dev/null || echo "none")
    last_action_ts=$(tail -1 "$WATCHDOG_STATE" 2>/dev/null || echo "0")
  fi

  # Check for recent gate pass or demo — role may be waiting for acceptance (#1891)
  if [ -f "$CHORUS_LOG" ] && [ -n "$CARD" ]; then
    # #2053: widened from tail -50 — high-activity sessions push gate events out fast
    recent_gate=$(tail -200 "$CHORUS_LOG" 2>/dev/null | grep -E "gate\.(code|quality|arch|ops)\.(passed|completed)|card\.demo\.started" | grep "card=$CARD" | tail -1 || true)
    if [ -n "$recent_gate" ]; then
      # Extract timestamp from the gate event and check if it's within threshold
      gate_ts=$(echo "$recent_gate" | python3 -c "
import sys,re
from datetime import datetime,timezone
line=sys.stdin.read()
m=re.search(r'\"timestamp\":\"([^\"]+)\"', line)
if m:
  ts=m.group(1)[:19]
  try:
    dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%S')
    import time; print(int(dt.replace(tzinfo=timezone.utc).timestamp()))
  except: print(0)
else: print(0)" 2>/dev/null || echo "0")
      gate_age=$((now - gate_ts))
      if [ "$gate_age" -lt "$ALERT_THRESHOLD" ]; then
        # Gate passed recently — role is waiting for acceptance, not stalled
        continue
      fi
    fi
  fi

  # Role is active recently — reset watchdog
  if [ "$effective_age" -lt "$OPS_NUDGE_THRESHOLD" ]; then
    if [ "$last_action" != "none" ]; then
      echo "none" > "$WATCHDOG_STATE"
      echo "$now" >> "$WATCHDOG_STATE"
    fi
    all_inactive=false
    continue
  fi

  all_inactive=false
  inactive_count=$((inactive_count + 1))

  # Level 1: Nudge the role (2min)
  if [ "$effective_age" -ge "$OPS_NUDGE_THRESHOLD" ] && [ "$last_action" = "none" ]; then
    bash "$OPS_NUDGE" "$role" "watchdog: no activity in $((effective_age / 60))min, are you blocked?" system 2>/dev/null || true
    echo "nudged" > "$WATCHDOG_STATE"
    echo "$now" >> "$WATCHDOG_STATE"
    echo "role.state.changed | system watchdog.nudge.sent role=$role age=${effective_age}s" >> "$CHORUS_LOG"
    continue
  fi

  # Level 2: Escalate to Wren (3min)
  if [ "$effective_age" -ge "$ESCALATE_THRESHOLD" ] && [ "$last_action" = "nudged" ]; then
    bash "$OPS_NUDGE" "$role" "watchdog: still no response after $((effective_age / 60))min" system 2>/dev/null || true
    bash "$OPS_NUDGE" wren "watchdog: $role unresponsive $((effective_age / 60))min on #${CARD}" system 2>/dev/null || true
    echo "escalated" > "$WATCHDOG_STATE"
    echo "$now" >> "$WATCHDOG_STATE"
    echo "role.state.changed | system watchdog.escalated role=$role age=${effective_age}s" >> "$CHORUS_LOG"
    continue
  fi

  # Level 3: Alert Jeff (5min)
  if [ "$effective_age" -ge "$ALERT_THRESHOLD" ] && [ "$last_action" = "escalated" ]; then
    MSG=$(printf 'watchdog: %s unresponsive %dmin on #%s. Nudged at 2min, escalated to Wren at 3min.' "$role" "$((effective_age / 60))" "$CARD")
    curl -sf -X POST "$BRIDGE" \
      -H 'Content-Type: application/json' \
      -d "$(jq -n --arg text "$MSG" --arg from "system" '{from: $from, text: $text}')" \
      &>/dev/null || true
    echo "alerted" > "$WATCHDOG_STATE"
    echo "$now" >> "$WATCHDOG_STATE"
    echo "role.state.changed | system watchdog.alert.jeff role=$role age=${effective_age}s" >> "$CHORUS_LOG"
  fi
done

# System-wide alert: all roles inactive
if [ "$inactive_count" -eq 3 ]; then
  COOLDOWN="/tmp/watchdog-all-inactive-$(date '+%Y-%m-%d-%H')"
  if [ ! -f "$COOLDOWN" ]; then
    touch "$COOLDOWN"
    MSG="watchdog: All roles inactive for 10+ minutes"
    curl -sf -X POST "$BRIDGE" \
      -H 'Content-Type: application/json' \
      -d "$(jq -n --arg text "$MSG" --arg from "system" '{from: $from, text: $text}')" \
      &>/dev/null || true
    echo "role.state.changed | system watchdog.all.inactive" >> "$CHORUS_LOG"
  fi
fi
