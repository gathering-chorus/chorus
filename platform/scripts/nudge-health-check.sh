#!/usr/bin/env bash
# Nudge health check — #4786 (Slice 3 of #4780).
# Judges delivery results and run liveness, not Terminal windows.
# Agrees with Pulse on who is online and catches refused nudges.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
CANARY_DIR="/tmp/nudge-canary"
mkdir -p "$CANARY_DIR" 2>/dev/null || true

log_canary() {
  { echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') $*" >> "${CANARY_DIR}/health.log"; } 2>/dev/null || true
}

PULSE_URL="${PULSE_URL:-http://localhost:3475}"
MESSAGES_DB="${CHORUS_MESSAGES_DB:-${CHORUS_ROOT}/platform/pulse/messages.db}"
SESSIONS_DIR="${CHORUS_SESSIONS_DIR:-/Users/jeffbridwell/.chorus/sessions}"

# Dynamic role discovery from roles door or fallback to canonical 5
ROLES=(wren silas kade borg-queen abby-normal)
if [ -n "${ROLES_OVERRIDE:-}" ]; then
  read -ra ROLES <<< "$ROLES_OVERRIDE"
elif [ -z "${CHORUS_SESSIONS_DIR:-}" ]; then
  DOOR_ROLES=$(curl -s --max-time 2 "${API_BASE:-http://localhost:3340}/v1/roles/roles" 2>/dev/null | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    agents = [r["name"] for r in d.get("data", []) if r.get("roleKind") == "agent"]
    if agents:
        print(" ".join(agents))
except Exception:
    pass
' 2>/dev/null || true)
  if [ -n "$DOOR_ROLES" ]; then
    read -ra ROLES <<< "$DOOR_ROLES"
  fi
fi

FAILURES=0

# Resolve a role to its most-recently-registered LIVE session.
resolve_reg() {
  python3 - "$1" "$SESSIONS_DIR" <<'PY'
import json, os, sys, glob
role = sys.argv[1]
sessions_dir = sys.argv[2]
best = None
for f in glob.glob(f"{sessions_dir}/{role}-*.json"):
    try:
        d = json.load(open(f))
    except Exception:
        continue
    pid = d.get("pid")
    if not pid:
        continue
    try:
        os.kill(int(pid), 0)          # alive?
    except ProcessLookupError:
        continue                       # dead — never a target (registry liveness)
    except PermissionError:
        pass                           # exists, not signalable — still alive
    except Exception:
        continue
    at = str(d.get("registered_at", "0"))
    if best is None or at > best[0]:
        best = (at, d.get("host", "unknown"), pid, d.get("tty", ""), d.get("tmux", ""))
if best:
    print(f"{best[1]}|{best[2]}|{best[3]}|{best[4]}")
PY
}

# Fetch online status from Pulse actor-state (if reachable)
pulse_actors=$(curl -s --max-time 2 "${PULSE_URL}/api/actor-state" 2>/dev/null || true)

# 1. Check recent nudge delivery errors from messages.db or test stub
failed_nudges=""
if [ -n "${STUB_FAILED_NUDGES:-}" ]; then
  failed_nudges="$STUB_FAILED_NUDGES"
elif [ -f "$MESSAGES_DB" ] && command -v sqlite3 >/dev/null 2>&1; then
  failed_nudges=$(sqlite3 "$MESSAGES_DB" "
    SELECT \"to\", last_delivery_error
    FROM messages
    WHERE type = 'nudge'
      AND delivery_status = 'failed'
      AND created_at >= datetime('now', '-1 hour')
    ORDER BY id DESC;
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

  # Check delivery path role support (chorus-inject / role_pattern check)
  if ! python3 -c "
import sys
KNOWN = {'wren', 'silas', 'kade', 'borg-queen', 'abby-normal'}
if sys.argv[1] not in KNOWN:
    sys.exit(1)
" "$role" 2>/dev/null; then
    echo "FAIL: ${role} — delivery path refuses role (unknown role: ${role})"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=unknown-role" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))
    log_canary "role=${role} status=FAIL error=unknown-role"
    continue
  fi

  reg="$(resolve_reg "$role" || true)"
  host="${reg%%|*}"
  rpid=""
  rpane=""
  [ -n "$reg" ] && { rest="${reg#*|}"; rpid="${rest%%|*}"; rpane="${reg##*|}"; }

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

  # Check tmux host
  if [ -n "$reg" ] && [ "$host" = "tmux" ]; then
    if [ -n "$rpane" ] && tmux list-panes -a -F '#{pane_id}' 2>/dev/null | grep -qx "$rpane"; then
      echo "OK: ${role} — tmux session (pid ${rpid}) alive, pane ${rpane} exists → paste-buffer reachable"
    else
      echo "ALERT: ${role} — tmux session (pid ${rpid}) registered but pane ${rpane:-<none>} NOT found — tmux delivery cannot land"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=tmux-pane-gone,pid=${rpid},pane=${rpane:-none}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    fi
    log_canary "role=${role} host=tmux pid=${rpid} pane=${rpane:-none}"
    continue
  fi

  # Check vscode host
  if [ -n "$reg" ] && [ "$host" = "vscode" ]; then
    if pgrep -x "Code" >/dev/null 2>&1 || pgrep -f "Visual Studio Code" >/dev/null 2>&1 \
       || pgrep -f "Code Helper" >/dev/null 2>&1; then
      echo "OK: ${role} — vscode session (pid ${rpid}) alive, Code running → --vscode reachable"
    else
      echo "ALERT: ${role} — vscode session (pid ${rpid}) registered but Code app is NOT running → --vscode cannot land"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=vscode-app-down,pid=${rpid}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    fi
    log_canary "role=${role} host=vscode pid=${rpid}"
    continue
  fi

  # Check live session on own account (terminal / agent / unknown host)
  if [ -n "$reg" ]; then
    echo "OK: ${role} — live session on host=${host} (pid ${rpid}), agreed with Pulse (pulse_online=${pulse_online})"
    log_canary "role=${role} host=${host} pid=${rpid}"
    continue
  fi

  # Role is not registered locally
  if [ "$pulse_online" = "true" ]; then
    echo "WARN: ${role} — Pulse says online but no live session registered locally (disagreement)"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.disagreement" "system" "role=${role},pulse=online,local=offline" 2>/dev/null || true
  else
    echo "OK: ${role} — offline (agreed with Pulse, no active session)"
  fi
  log_canary "role=${role} host=none status=offline"
done

if [ "$FAILURES" -gt 0 ]; then
  echo "NUDGE HEALTH: ${FAILURES} role(s) have issues"
  exit 1
else
  echo "NUDGE HEALTH: all roles reachable"
  exit 0
fi
