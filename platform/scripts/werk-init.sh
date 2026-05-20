#!/bin/bash
# werk-init.sh — Lean session lifecycle script (#1590 rewrite)
#
# Modes:
#   werk-init.sh <role>            — Interactive: show cached context
#   werk-init.sh <role> --scan     — Per-turn: nudge drain + brief check (UserPromptSubmit hook)
#   werk-init.sh <role> --close    — Close-out introspection
#
# Session boot is handled by session-start-thin.sh (25 lines).
# Context assembly is handled by context-cache-5min.sh (LaunchAgent).
# This file handles scan, close, and interactive modes only.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

ROLE=""
CLOSE_MODE=false
SCAN_MODE=false
for arg in "$@"; do
  case "$arg" in
    --session) echo "DEPRECATED: use session-start-thin.sh instead" >&2; exit 1 ;;
    --close)   CLOSE_MODE=true ;;
    --scan)    SCAN_MODE=true ;;
    *)         [ -z "$ROLE" ] && ROLE="$arg" ;;
  esac
done

if [ -z "$ROLE" ]; then
  echo "Usage: werk-init.sh <role> [--scan|--close]" >&2
  exit 1
fi

case "$ROLE" in
  wren)  ROLE_DIR="$REPO_ROOT/roles/wren"; ROLE_DIR_NAME="wren" ;;
  silas) ROLE_DIR="$REPO_ROOT/roles/silas"; ROLE_DIR_NAME="silas" ;;
  kade)  ROLE_DIR="$REPO_ROOT/roles/kade"; ROLE_DIR_NAME="kade" ;;
  *)     echo "Unknown role: $ROLE" >&2; exit 1 ;;
esac

BOARD_TS="$SCRIPT_DIR/cards"

# ============================================================
# HELPER: Dedup gate for auto-error card creation (#1455)
# ============================================================
auto_error_dedup() {
  local FP="$1" SESS_COUNT="$2" SAMPLE_CMD="$3" SOURCE="${4:-boot}"
  local STATE_FILE="$HOME/.chorus/auto-error-carded.txt"
  mkdir -p "$HOME/.chorus"
  touch "$STATE_FILE"
  DEDUP_RESULT="skipped"

  local EXISTING
  EXISTING=$("$BOARD_TS" list 2>/dev/null | grep "\[auto-error\].*${FP}" || true)

  if [ -n "$EXISTING" ]; then
    local CARD_ID
    CARD_ID=$(echo "$EXISTING" | head -1 | awk '{print $1}')
    local IS_DONE=false
    if echo "$EXISTING" | head -1 | grep -qE '^\s*Done|Won.*Do'; then
      IS_DONE=true
    elif "$BOARD_TS" view "$CARD_ID" 2>/dev/null | grep -q 'Status:.*Done\|Status:.*Won'; then
      IS_DONE=true
    fi

    if [ "$IS_DONE" = true ]; then
      local CARD_RESULT
      CARD_RESULT=$("$BOARD_TS" add "[auto-error] Recurring: ${FP} — ${SESS_COUNT} sessions" \
        --owner Silas --priority P2 --domain infrastructure --chunk ops \
        --desc "Auto-carded by werk-init ${SOURCE}. Fingerprint ${FP} across ${SESS_COUNT} sessions. Sample: ${SAMPLE_CMD}" 2>&1 || true)
      local NEW_ID
      NEW_ID=$(echo "$CARD_RESULT" | grep -oE '#[0-9]+' | head -1)
      [ -n "$NEW_ID" ] && { echo "$FP" >> "$STATE_FILE"; DEDUP_RESULT="created $NEW_ID"; } || DEDUP_RESULT="failed"
    else
      "$BOARD_TS" comment "$CARD_ID" "Auto-error recurring: ${FP} now ${SESS_COUNT} sessions. Sample: ${SAMPLE_CMD}" 2>/dev/null || true
      grep -qx "$FP" "$STATE_FILE" 2>/dev/null || echo "$FP" >> "$STATE_FILE"
      DEDUP_RESULT="commented #${CARD_ID}"
    fi
  elif [ "$SESS_COUNT" -ge 3 ]; then
    local CARD_RESULT
    CARD_RESULT=$("$BOARD_TS" add "[auto-error] Recurring: ${FP} — ${SESS_COUNT} sessions" \
      --owner Silas --priority P2 --domain infrastructure --chunk ops \
      --desc "Auto-carded by werk-init ${SOURCE}. Fingerprint ${FP} across ${SESS_COUNT} sessions. Sample: ${SAMPLE_CMD}" 2>&1 || true)
    local NEW_ID
    NEW_ID=$(echo "$CARD_RESULT" | grep -oE '#[0-9]+' | head -1)
    [ -n "$NEW_ID" ] && { echo "$FP" >> "$STATE_FILE"; DEDUP_RESULT="created $NEW_ID"; } || DEDUP_RESULT="failed"
  fi
}

# ============================================================
# SCAN MODE — per-turn nudge drain + brief check
# ============================================================
if $SCAN_MODE; then
  CACHE_DIR="${CACHE_DIR:-/tmp/claude-team-scan}"
  BRIEFS_DIR="${ROLE_DIR}/briefs"
  mkdir -p "$CACHE_DIR"

  echo "$(date +%s)" >> "$CACHE_DIR/${ROLE}-prompt-times.log"
  "$SCRIPT_DIR/wall-clock" --write >/dev/null 2>&1 || true

  # Session PID for andon
  _CPID=$PPID
  for _ in 1 2 3 4 5; do
    _PNAME=$(ps -o comm= -p "$_CPID" 2>/dev/null | xargs)
    if [[ "$_PNAME" == "claude" ]]; then
      echo "$_CPID" > "$CACHE_DIR/${ROLE}.pid"; break
    fi
    _CPID=$(ps -o ppid= -p "$_CPID" 2>/dev/null | tr -d ' ')
    [ -z "$_CPID" ] || [ "$_CPID" -le 1 ] && break
  done

  # Drain nudges from messaging tier (DEC-107)
  NUDGE_OUTPUT=""
  NUDGE_OUTPUT=$("$SCRIPT_DIR/nudge" drain "$ROLE" 2>/dev/null) || true
  # Note: nudge drain already acks + emits spine event
  # Print nudges immediately — before rate limiter can exit
  if [ -n "$NUDGE_OUTPUT" ]; then
    echo "<team-scan>"
    echo "$NUDGE_OUTPUT"
    echo "</team-scan>"
  fi

  # Rate limiting (briefs + version check)
  LAST_SCAN_FILE="$CACHE_DIR/${ROLE}-last-scan"
  NOW=$(date +%s)
  if [ -f "$LAST_SCAN_FILE" ]; then
    LAST_SCAN=$(cat "$LAST_SCAN_FILE")
    [ $((NOW - LAST_SCAN)) -lt 30 ] && exit 0
  fi
  echo "$NOW" > "$LAST_SCAN_FILE"

  # Brief check
  BRIEF_OUTPUT=""
  if [ -d "$BRIEFS_DIR" ]; then
    SCAN_MARKER="$CACHE_DIR/${ROLE}-brief-marker"
    [ -f "$SCAN_MARKER" ] || touch -t 202601010000 "$SCAN_MARKER"
    NEW_BRIEFS=$(find "$BRIEFS_DIR" -name "*.md" -newer "$SCAN_MARKER" 2>/dev/null | head -10)
    if [ -n "$NEW_BRIEFS" ]; then
      BRIEF_OUTPUT="New briefs in inbox:"
      for BRIEF in $NEW_BRIEFS; do
        BRIEF_OUTPUT="${BRIEF_OUTPUT}
${BRIEF}

--- $(basename "$BRIEF") ---"
        _content=$(head -20 "$BRIEF" 2>/dev/null || true)
        BRIEF_OUTPUT="${BRIEF_OUTPUT}
${_content}"
        LINES=$(wc -l < "$BRIEF" 2>/dev/null || echo 0)
        [ "$LINES" -gt 20 ] && BRIEF_OUTPUT="${BRIEF_OUTPUT}
... ($(( LINES - 20 )) more lines — read full file)"
      done
      touch "$SCAN_MARKER"
    fi
  fi

  # Version check
  VERSION_WARNING=""
  MANIFEST="$SCRIPT_DIR/../claudemd/manifest.json"
  ACK_FILE="$CACHE_DIR/${ROLE}-protocol-version"
  if [ -f "$MANIFEST" ]; then
    CURRENT_VERSION=$(grep '"version"' "$MANIFEST" | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/')
    ACKED_VERSION=""
    [ -f "$ACK_FILE" ] && ACKED_VERSION=$(cat "$ACK_FILE")
    [ "$CURRENT_VERSION" != "$ACKED_VERSION" ] && VERSION_WARNING="⚠ PROTOCOL UPDATE: v${ACKED_VERSION:-unknown} → v${CURRENT_VERSION}. Re-read team-architecture.md."
  fi

  # Card events from chorus.log — filtered by pulse level (#1896)
  CARD_EVENTS=""
  CHORUS_LOG="${CHORUS_ROOT}/platform/logs/chorus.log"
  if [ -f "$CHORUS_LOG" ]; then
    CARD_EVENTS=$(tail -500 "$CHORUS_LOG" | python3 -c "
import sys, json
events = []
for line in sys.stdin:
    try:
        d = json.loads(line.strip())
        evt = d.get('event', '')
        if not evt.startswith('card.') and not evt.startswith('icd.'): continue
        role = d.get('role', '?')
        level = d.get('level', 'info')
        card = d.get('card', '')
        ts = d.get('timestamp', '')[:16]
        label = f'[{level.upper()}] ' if level != 'info' else ''
        if card:
            events.append(f'{label}{evt} | {role} #{card}')
        else:
            events.append(f'{label}{evt} | {role}')
    except: pass
# Show last 5, critical first
events = events[-5:]
print('\n'.join(events))
" 2>/dev/null || true)
  fi

  # Cross-role observation glance — ambient gemba
  GLANCE=""
  for OTHER_ROLE in wren silas kade; do
    [ "$OTHER_ROLE" = "$ROLE" ] && continue
    OBS_FILE="$CACHE_DIR/${OTHER_ROLE}-observations.jsonl"
    STATE_FILE="$CACHE_DIR/${OTHER_ROLE}-declared.json"
    # Get andon state
    OTHER_STATE=""
    OTHER_CARD=""
    if [ -f "$STATE_FILE" ]; then
      OTHER_STATE=$(python3 -c "import json,sys; d=json.load(open('$STATE_FILE')); print(d.get('state','?'))" 2>/dev/null || echo "?")
      OTHER_CARD=$(python3 -c "import json,sys; d=json.load(open('$STATE_FILE')); c=d.get('card',''); print(f'#{c}' if c else '')" 2>/dev/null || true)
      # Staleness detection (#2031, #2224) — flag if no state change in 45min AND no recent tool activity
      # Check observation recency first — active roles have recent observations even with stale state declarations
      OBS_AGE=9999
      if [ -f "$OBS_FILE" ]; then
        OBS_AGE=$(tail -1 "$OBS_FILE" 2>/dev/null | python3 -c "
import json,sys,time
from datetime import datetime,timezone
try:
  o=json.load(sys.stdin); ts=o.get('ts','')
  if ts.endswith('Z'): dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)
  elif '-' in ts[-5:] or '+' in ts[-5:]: dt=datetime.fromisoformat(ts)
  else: dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%S').replace(tzinfo=timezone.utc)
  print(max(0,int(time.time()-dt.timestamp())))
except: print(9999)" 2>/dev/null || echo 9999)
      fi
      # Only stale if BOTH state declaration >45min AND last observation >5min
      if [ "$OBS_AGE" -lt 300 ]; then
        OTHER_STALE=""  # Active tool calls in last 5min — not stale
      else
        OTHER_STALE=$(python3 -c "import json,time; d=json.load(open('$STATE_FILE')); ts=d.get('ts',0); age=int(time.time())-int(ts); print('[STALE]' if age>2700 else '')" 2>/dev/null || true)
      fi
    fi
    # Get last observation
    LAST_OBS=""
    if [ -f "$OBS_FILE" ]; then
      LAST_LINE=$(tail -1 "$OBS_FILE" 2>/dev/null)
      if [ -n "$LAST_LINE" ]; then
        LAST_OBS=$(echo "$LAST_LINE" | python3 -c "
import json,sys,time
try:
  o=json.load(sys.stdin)
  ts=o.get('ts','')
  from datetime import datetime,timezone,timedelta
  try:
    # Handle both UTC (Z) and Eastern offset (-0400/-0500) formats
    if ts.endswith('Z'):
      dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)
    elif '-' in ts[-5:] or '+' in ts[-5:]:
      dt=datetime.fromisoformat(ts)
    else:
      dt=datetime.strptime(ts,'%Y-%m-%dT%H:%M:%S').replace(tzinfo=timezone.utc)
    age_s=int(time.time()-dt.timestamp())
    if age_s<0: age_s=0
    if age_s<60: age=f'{age_s}s ago'
    elif age_s<3600: age=f'{age_s//60}m ago'
    else: age=f'{age_s//3600}h ago'
  except: age='?'
  d=o.get('digest','')
  print(f'{d} ({age})')
except: pass" 2>/dev/null || true)
      fi
    fi
    if [ -n "$OTHER_STATE" ] && [ "$OTHER_STATE" != "?" ]; then
      LINE="${OTHER_ROLE}: ${OTHER_STALE:+${OTHER_STALE} }${OTHER_STATE}${OTHER_CARD:+ ${OTHER_CARD}}"
      if [ -n "$LAST_OBS" ]; then
        LINE="${LINE} — ${LAST_OBS}"
      else
        # No recent observation but state declared — check if session PID is alive
        OTHER_PID=$(python3 -c "import json; d=json.load(open('$STATE_FILE')); print(d.get('pid',''))" 2>/dev/null || true)
        if [ -n "$OTHER_PID" ] && kill -0 "$OTHER_PID" 2>/dev/null; then
          LINE="${LINE} — (thinking)"
        fi
      fi
      GLANCE="${GLANCE:+${GLANCE}
}${LINE}"
    fi
  done

  if [ -n "$BRIEF_OUTPUT" ] || [ -n "$VERSION_WARNING" ] || [ -n "$GLANCE" ] || [ -n "$CARD_EVENTS" ]; then
    echo "<team-scan>"
    [ -n "$VERSION_WARNING" ] && echo "$VERSION_WARNING" && echo ""
    [ -n "$GLANCE" ] && echo "$GLANCE" && echo ""
    [ -n "$CARD_EVENTS" ] && echo "$CARD_EVENTS" && echo ""
    [ -n "$BRIEF_OUTPUT" ] && echo "$BRIEF_OUTPUT"
    echo "</team-scan>"
  fi
  exit 0
fi

# ============================================================
# CLOSE MODE — thin wrapper (#1597)
# Recurring checks moved to cron tiers (hourly/daily). Only synchronous steps remain.
# ============================================================
if $CLOSE_MODE; then
  exec "$SCRIPT_DIR/session-close-thin.sh" "$ROLE"
fi

# ============================================================
# INTERACTIVE MODE — show cached context
# ============================================================
CACHE="/tmp/session-context-${ROLE}.md"
if [ -f "$CACHE" ]; then
  cat "$CACHE"
else
  echo "No cached context. Run: context-cache-5min.sh $ROLE"
fi
