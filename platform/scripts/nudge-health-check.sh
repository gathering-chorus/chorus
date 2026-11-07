#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/lib/boston-time.sh" 2>/dev/null || . "${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/platform/scripts/lib/boston-time.sh"  # boston_time: the one Boston renderer (#4885)
# Nudge health check — #4786 (Slice 3 of #4780).
# Judges delivery results and run liveness, not Terminal windows.
# Agrees with Pulse on who is online and catches refused nudges.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
CANARY_DIR="${CANARY_DIR:-/tmp/nudge-canary}"
# Fall back to user-scoped canary dir if shared health.log is unwritable
[ -e "${CANARY_DIR}/health.log" ] && [ ! -w "${CANARY_DIR}/health.log" ] && CANARY_DIR="${CANARY_DIR}-${USER:-$(id -un)}"
mkdir -p "$CANARY_DIR"

log_canary() {
  echo "$(boston_time stamp) $*" >> "${CANARY_DIR}/health.log"
}

PULSE_URL="${PULSE_URL:-http://localhost:3475}"
MESSAGES_DB="${CHORUS_MESSAGES_DB:-${MESSAGES_DB:-${CHORUS_ROOT}/platform/pulse/messages.db}}"
STATE_DIR="${CHORUS_STATE_DIR:-/Users/jeffbridwell/.chorus}"

ROLES_URL="${CHORUS_ROLES_URL:-http://localhost:3360/v1/roles/roles?limit=500}"
DOOR_ROLES=$(python3 -c '
import json, sys, urllib.request
url = sys.argv[1]
try:
    req = urllib.request.urlopen(url, timeout=2)
    d = json.loads(req.read().decode())
    agents = [r["name"] for r in d.get("data", []) if r.get("roleKind") == "agent"]
    if agents:
        print(" ".join(sorted(agents)))
        sys.exit(0)
except Exception:
    pass
sys.exit(1)
' "$ROLES_URL" 2>/dev/null || true)

if [ -z "$DOOR_ROLES" ]; then
  echo "UNMEASURED — roles door is unreadable ($ROLES_URL)"
  exit 2
fi

# Dynamic role discovery from roles door (no hardcoded fallback)
if [ -n "${ROLES_OVERRIDE:-}" ]; then
  read -ra ROLES <<< "$ROLES_OVERRIDE"
else
  read -ra ROLES <<< "$DOOR_ROLES"
fi

FAILURES=0
UNMEASURED_ROLES=0

# #4895 / #4916 — where a nudge to the role goes: pulse's own answer (/api/where/<role>,
# the resolution delivery uses: the live run's Presence with reachability). Prints
# kind|pane|reachability|why; an unreadable source prints unread|||why.
# #4916: direct tmux inspection retired (Jeff 11-05; non-jeff accounts lack socket access).
# Silas 16:50 note: gap to name: a dead pane nobody nudges stays "reachable" (Pulse reachability
# is cached until the next delivery attempt).
WHERE_URL="${WHERE_URL:-${PULSE_URL}/api/where}"
where_of() {
  local body
  body=$(curl -sf --max-time 3 "${WHERE_URL}/$1" 2>/dev/null) || { echo "unread|||${WHERE_URL}/$1 did not answer"; return; }
  printf '%s' "$body" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    print("unread|||not JSON"); sys.exit(0)
print("|".join([str(d.get("kind", "unread")), str(d.get("pane", "")), str(d.get("reachability", "")), str(d.get("why", d.get("run", "")))]))'
}

# Fetch online status from Pulse actor-state (must be reachable, otherwise UNMEASURED)
if [ -z "${pulse_actors:-}" ]; then
  pulse_actors=$(curl -s --max-time 2 "${PULSE_URL}/api/actor-state" 2>/dev/null || true)
fi
if [ -z "$pulse_actors" ] || ! echo "$pulse_actors" | grep -q '"actors"'; then
  echo "UNMEASURED — Pulse actor-state is unreadable (${PULSE_URL}/api/actor-state)"
  exit 2
fi

# 1. Check recent nudge delivery errors from messages.db or test stub
failed_nudges=""
if [ -n "${STUB_FAILED_NUDGES:-}" ]; then
  failed_nudges="$STUB_FAILED_NUDGES"
elif [ -f "$MESSAGES_DB" ] && command -v sqlite3 >/dev/null 2>&1; then
  failed_nudges=$(sqlite3 "$MESSAGES_DB" "
    WITH latest_nudges AS (
      SELECT \"to\", delivery_status, last_delivery_error,
             ROW_NUMBER() OVER (PARTITION BY \"to\" ORDER BY id DESC) as rn
      FROM messages
      WHERE type = 'nudge'
        AND created_at >= datetime('now', '-24 hours')
    )
    SELECT \"to\", last_delivery_error
    FROM latest_nudges
    WHERE rn = 1 AND delivery_status = 'failed';
  " 2>/dev/null || true)
fi

for role in "${ROLES[@]}"; do
  # Check if a recent nudge to this role failed (e.g. unknown role or delivery failure)
  role_fail=$(echo "$failed_nudges" | grep -F "${role}|" | head -1 || true)
  if [ -n "$role_fail" ]; then
    err_msg="${role_fail#*|}"
    echo "FAIL: ${role} — nudge delivery failed: ${err_msg}"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=delivery-failed,error=${err_msg}" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))
    log_canary "role=${role} status=FAIL error=${err_msg}"
    continue
  fi

  # Check delivery path role support (chorus-inject / role_pattern check).
  # Uses the door-discovered roles parsed at startup, never a hardcoded list (#4432).
  is_known=false
  for kr in $DOOR_ROLES; do
    if [ "$kr" = "$role" ]; then
      is_known=true
      break
    fi
  done
  if [ "$is_known" = "false" ]; then
    echo "FAIL: ${role} — delivery path refuses role (unknown role: ${role})"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=unknown-role" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))
    log_canary "role=${role} status=FAIL error=unknown-role"
    continue
  fi

  where="$(where_of "$role")"
  IFS='|' read -r kind rpane reachability why <<< "$where"

  if [ "$kind" = "unread" ]; then
    echo "UNMEASURED — where/${role} is unreadable (${why})"
    UNMEASURED_ROLES=$((UNMEASURED_ROLES + 1))
    log_canary "role=${role} where=unread"
    continue
  fi

  # Pulse online state for this role
  pulse_online="unknown"
  if [ -n "$pulse_actors" ]; then
    pulse_online=$(python3 - "$role" <<PY
import json, sys
role = sys.argv[1]
try:
    data = json.loads('''$pulse_actors''')
    for a in data.get("actors", []):
        p = a.get("aboutPrincipal", "")
        if p == f"principal-{role}" or a.get("name", "").lower() == role:
            live = any(s.get("live", False) for s in a.get("sessions", []))
            print("true" if live else "false")
            sys.exit(0)
    print("false")
except Exception:
    print("unknown")
PY
    )
  fi

  case "$kind" in
    resolved)
      if [ "$reachability" = "reachable" ]; then
        echo "OK: ${role} — live run resolved and reachable (${rpane})"
      elif [ "$reachability" = "unknown" ] || [ -z "$reachability" ]; then
        echo "UNMEASURED: ${role} — reachability unknown (not yet delivered)"
        UNMEASURED_ROLES=$((UNMEASURED_ROLES + 1))
      else
        echo "ALERT: ${role} — live run on pane ${rpane:-<none>} is unreachable"
        [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=unreachable,pane=${rpane:-none}" 2>/dev/null || true
        FAILURES=$((FAILURES + 1))
      fi
      log_canary "role=${role} where=pane pane=${rpane:-none}" ;;
    agent)
      echo "OK: ${role} — live run reached over the agent channel (run ${why})"
      log_canary "role=${role} where=agent run=${why}" ;;
    no-pane)
      echo "WARN: ${role} — live run has no tmux pane; nudges wait for its turn-end drain"
      log_canary "role=${role} where=no-pane" ;;
    ambiguous)
      echo "ALERT: ${role} — live on both a pane run and an agent run; delivery has no single answer"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=ambiguous" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
      log_canary "role=${role} where=ambiguous" ;;
    dead|unregistered)
      if [ "$pulse_online" = "true" ]; then
        echo "WARN: ${role} — Pulse says online but no live run has a Presence (disagreement)"
        [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.disagreement" "system" "role=${role},pulse=online,presence=${kind}" 2>/dev/null || true
      else
        echo "OK: ${role} — offline (agreed with Pulse, no live run)"
      fi
      log_canary "role=${role} where=none status=offline" ;;
    *)
      echo "UNMEASURED: ${role} — where it is could not be read (${why:-$kind})"
      UNMEASURED_ROLES=$((UNMEASURED_ROLES + 1))
      log_canary "role=${role} where=unread" ;;
  esac
done

if [ "$FAILURES" -gt 0 ]; then
  echo "NUDGE HEALTH: ${FAILURES} role(s) have issues"
  exit 1
elif [ "$UNMEASURED_ROLES" -gt 0 ]; then
  echo "NUDGE HEALTH: ${UNMEASURED_ROLES} role(s) unmeasured"
  exit 2
else
  echo "NUDGE HEALTH: all roles reachable"
  exit 0
fi
