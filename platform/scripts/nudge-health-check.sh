#!/usr/bin/env bash
# Nudge health check — AC4/AC5 for #1793
# For each role:
#   1. Finds the correct window (pattern + claude in name)
#   2. Verifies exactly ONE window matches (no ambiguity = no wrong-terminal)
#   3. Verifies the matched window name is for the CORRECT role (canary check)
# Alerts on failure via chorus-log spine event + Bridge.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
ROLES=(wren silas kade)
FAILURES=0
CANARY_DIR="/tmp/nudge-canary"
mkdir -p "$CANARY_DIR"

for role in "${ROLES[@]}"; do
  case "$role" in
    wren)  pattern="product-manager" ;;
    silas) pattern="architect" ;;
    kade)  pattern="engineer" ;;
  esac

  # Find matching windows — return count + the actual window name that matched
  result=$(osascript -e "
tell application \"Terminal\"
    set matchCount to 0
    set matchName to \"\"
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        set winName to name of w
        if winName contains \"${pattern}\" and winName contains \"claude\" then
            set matchCount to matchCount + 1
            set matchName to winName
        end if
    end repeat
    return (matchCount as text) & \"::\" & matchName
end tell" 2>&1)

  match_count=$(echo "$result" | cut -d':' -f1)
  match_name=$(echo "$result" | sed 's/^[0-9]*:://')

  if [ "$match_count" = "0" ]; then
    echo "ALERT: ${role} — no terminal window found (need ${pattern} + claude)"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.failed" "system" "role=${role},reason=no-window" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))

  elif [ "$match_count" != "1" ]; then
    echo "WARN: ${role} — ${match_count} windows match (ambiguous — inject may hit wrong one)"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.ambiguous" "system" "role=${role},count=${match_count}" 2>/dev/null || true
    FAILURES=$((FAILURES + 1))

  else
    # Canary check: verify the matched window is actually for the right role
    # The window name should contain the role's pattern. Check it doesn't
    # ALSO match another role's pattern (wrong-target detection).
    wrong_target=false
    case "$role" in
      wren)
        echo "$match_name" | grep -q "architect" && wrong_target=true
        echo "$match_name" | grep -q "engineer" && wrong_target=true
        ;;
      silas)
        echo "$match_name" | grep -q "product-manager" && wrong_target=true
        echo "$match_name" | grep -q "engineer" && wrong_target=true
        ;;
      kade)
        echo "$match_name" | grep -q "product-manager" && wrong_target=true
        echo "$match_name" | grep -q "architect" && wrong_target=true
        ;;
    esac

    if [ "$wrong_target" = true ]; then
      echo "ALERT: ${role} — matched window '${match_name}' contains another role's pattern — WRONG TARGET"
      [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "nudge.health.wrong-target" "system" "role=${role},window=${match_name}" 2>/dev/null || true
      FAILURES=$((FAILURES + 1))
    else
      echo "OK: ${role} — 1 window, correct target: ${match_name}"
    fi
  fi

  # Write canary result for audit
  echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') role=${role} count=${match_count} name=${match_name}" >> "${CANARY_DIR}/health.log"
done

if [ "$FAILURES" -gt 0 ]; then
  echo "NUDGE HEALTH: ${FAILURES} role(s) have issues"
  # Alert to Bridge
  curl -s -X POST http://localhost:3470/api/message \
    -H 'Content-Type: application/json' \
    -d "{\"from\": \"system\", \"text\": \"[ALERT] Nudge health check: ${FAILURES} role(s) unreachable or ambiguous. Check /tmp/nudge-canary/health.log\"}" \
    > /dev/null 2>&1 || true
  exit 1
else
  echo "NUDGE HEALTH: all roles reachable (1 window each, correct targets)"
  exit 0
fi
