#!/usr/bin/env bash
# Nudge health check — AC4/AC5 for #1793; registry-aware per ADR-039 / #3284 AC8.
#
# The router is the session registry ({role,pid,tty,host}); the protocol is
# host-selected active osascript (terminal/iterm → --tty; vscode → --vscode).
# So health MUST resolve through the registry, NOT blindly probe Terminal:
#   - host=vscode      → delivery is --vscode (Code app, NOT a Terminal window).
#                        Healthy iff the session is alive + Code is running.
#                        NEVER emit no-window for a vscode session (the 84× false
#                        alarm this fixes).
#   - host=terminal/iterm, or NO registration (legacy name-match fallback)
#                      → the window-pattern probe below is the right check.
# A real no-window is reserved for a terminal-host/fallback session with no
# matching window — and it stays loud + actionable.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
ROLES=(wren silas kade)
FAILURES=0
CANARY_DIR="/tmp/nudge-canary"
mkdir -p "$CANARY_DIR"

# Resolve a role to its most-recently-registered LIVE session.
# Prints "host|pid|tty|tmux" or "" (no live registration → window-pattern fallback).
# CHORUS_SESSIONS_DIR overrides the registry location (tests bring their own world).
resolve_reg() {
  python3 - "$1" <<'PY'
import json, os, sys, glob
role = sys.argv[1]
best = None
sessions_dir = os.environ.get("CHORUS_SESSIONS_DIR", os.path.expanduser("~/.chorus/sessions"))
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

for role in "${ROLES[@]}"; do
  case "$role" in
    wren)  pattern="wren" ;;
    silas) pattern="silas" ;;
    kade)  pattern="kade" ;;
  esac

  reg="$(resolve_reg "$role" || true)"
  host="${reg%%|*}"
  rpid=""
  rpane=""
  [ -n "$reg" ] && { rest="${reg#*|}"; rpid="${rest%%|*}"; rpane="${reg##*|}"; }

  # --- tmux: delivery is the app-level tmux paste (#3668). Do NOT Terminal-probe;
  # the pane is the address. no-window stays reserved for terminal hosts (ADR-039). ---
  if [ -n "$reg" ] && [ "$host" = "tmux" ]; then
    if [ -n "$rpane" ] && tmux list-panes -a -F '#{pane_id}' 2>/dev/null | grep -qx "$rpane"; then
      echo "OK: ${role} — tmux session (pid ${rpid}) alive, pane ${rpane} exists → paste-buffer reachable"
    else
      echo "ALERT: ${role} — tmux session (pid ${rpid}) registered but pane ${rpane:-<none>} NOT found — tmux delivery cannot land"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=tmux-pane-gone,pid=${rpid},pane=${rpane:-none}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    fi
    echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') role=${role} host=tmux pid=${rpid} pane=${rpane:-none}" >> "${CANARY_DIR}/health.log"
    continue
  fi

  # --- vscode: delivery is --vscode (Code app). Do NOT Terminal-probe. ---
  if [ -n "$reg" ] && [ "$host" = "vscode" ]; then
    if pgrep -x "Code" >/dev/null 2>&1 || pgrep -f "Visual Studio Code" >/dev/null 2>&1 \
       || pgrep -f "Code Helper" >/dev/null 2>&1; then
      echo "OK: ${role} — vscode session (pid ${rpid}) alive, Code running → --vscode reachable"
    else
      echo "ALERT: ${role} — vscode session (pid ${rpid}) registered but Code app is NOT running → --vscode cannot land"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=vscode-app-down,pid=${rpid}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    fi
    echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') role=${role} host=vscode pid=${rpid}" >> "${CANARY_DIR}/health.log"
    continue
  fi

  # --- terminal/iterm/unknown, or no registration → window-pattern probe ---
  # try/on error skips zombie windows Terminal can't name (-1728)
  result=$(osascript -e "
tell application \"Terminal\"
    set matchCount to 0
    set matchName to \"\"
    set winCount to count of windows
    repeat with i from 1 to winCount
        try
            set w to window i
            set winName to name of w
            if winName contains \"${pattern}\" and winName contains \"claude\" then
                set matchCount to matchCount + 1
                set matchName to winName
            end if
        end try
    end repeat
    return (matchCount as text) & \"::\" & matchName
end tell" 2>&1)

  match_count=$(echo "$result" | cut -d':' -f1)
  match_name=$(echo "$result" | sed 's/^[0-9]*:://')

  if [ "$match_count" = "0" ]; then
    if [ -n "$reg" ]; then
      echo "ALERT: ${role} — registered host=${host} (pid ${rpid}) but NO matching Terminal window — session moved or window retitled (need '${pattern}' + 'claude')"
    else
      echo "ALERT: ${role} — no registration AND no Terminal window (need '${pattern}' + 'claude') — session not started or registry stale"
    fi
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=no-window,host=${host:-none}" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))

  elif [ "$match_count" != "1" ]; then
    echo "WARN: ${role} — ${match_count} windows match (ambiguous — inject may hit wrong one)"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.ambiguous" "system" "role=${role},count=${match_count}" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))

  else
    wrong_target=false
    case "$role" in
      wren)  echo "$match_name" | grep -q "silas" && wrong_target=true; echo "$match_name" | grep -q "kade" && wrong_target=true ;;
      silas) echo "$match_name" | grep -q "wren" && wrong_target=true; echo "$match_name" | grep -q "kade" && wrong_target=true ;;
      kade)  echo "$match_name" | grep -q "wren" && wrong_target=true; echo "$match_name" | grep -q "silas" && wrong_target=true ;;
    esac
    if [ "$wrong_target" = true ]; then
      echo "ALERT: ${role} — matched window '${match_name}' contains another role's pattern — WRONG TARGET"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.wrong-target" "system" "role=${role},window=${match_name}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    else
      echo "OK: ${role} — 1 window, correct target: ${match_name}"
    fi
  fi

  echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') role=${role} host=${host:-none} count=${match_count} name=${match_name}" >> "${CANARY_DIR}/health.log"
done

if [ "$FAILURES" -gt 0 ]; then
  echo "NUDGE HEALTH: ${FAILURES} role(s) have issues"
  curl -s -X POST http://localhost:3470/api/message \
    -H 'Content-Type: application/json' \
    -d "{\"from\": \"system\", \"text\": \"[ALERT] Nudge health check: ${FAILURES} role(s) unreachable or ambiguous. Check /tmp/nudge-canary/health.log\"}" \
    > /dev/null 2>&1 || true
  exit 1
else
  echo "NUDGE HEALTH: all roles reachable (registry-resolved: vscode via --vscode, terminal via window)"
  exit 0
fi
