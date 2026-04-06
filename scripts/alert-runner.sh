#!/usr/bin/env bash
# alert-runner.sh — Execute all YAML alert checks and fire actions on failure
# Card #2000 | Silas
#
# Reads alert rules from chorus/alerting/*.yml, runs each check script,
# fires the action script if check fails.
#
# Usage: alert-runner.sh [--rule <name>]

set -euo pipefail

ALERT_DIR="/Users/jeffbridwell/CascadeProjects/chorus/alerting"
LOG="/Users/jeffbridwell/Library/Logs/Chorus/alert-runner.log"
TIMESTAMP() { TZ=America/New_York date '+%Y-%m-%d %H:%M:%S'; }

mkdir -p "$(dirname "$LOG")"

log() { echo "[$(TIMESTAMP)] $*" >> "$LOG"; }

run_check() {
  local rule_file="$1"
  local name
  name=$(grep '^name:' "$rule_file" | head -1 | sed 's/name: *//')

  local check_script
  check_script=$(awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")

  if [[ -z "$check_script" ]]; then
    log "SKIP $name — no check block"
    return
  fi

  local result
  result=$(bash -c "$check_script" 2>&1) || true

  if [[ "$result" == "ok" ]]; then
    log "OK $name"
    return
  fi

  log "FIRE $name — $result"

  local action_script
  action_script=$(awk '/^action: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")

  if [[ -n "$action_script" ]]; then
    bash -c "$action_script" >> "$LOG" 2>&1 || true
    log "  ACTION $name fired"
  fi

  # DEC-107: persist AND deliver. Action posts to Bridge (persist).
  # Now also nudge --force to owning role's terminal (deliver). (#2037)
  # Nudge cooldown: max once per 10 minutes per alert, same as action (#2071 fix)
  local nudge_cooldown="/tmp/alert-nudge-${name}-$(date '+%Y-%m-%d-%H')-$(( 10#$(date '+%M') / 10 ))"
  if [[ -f "$nudge_cooldown" ]]; then
    log "  NUDGE $name cooldown (skipped)"
  else
    touch "$nudge_cooldown"
    local owner
    owner=$(grep '^owner:' "$rule_file" | head -1 | sed 's/owner: *//' || true)
    owner="${owner:-silas}"
    local alert_ts
    alert_ts=$(TZ=America/New_York date '+%Y-%m-%d %H:%M')
    local nudge_msg="Alert — $alert_ts | $name: $result"
    bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge "$owner" "$nudge_msg" --force >> "$LOG" 2>&1 || true
    log "  NUDGE $owner ($name)"
  fi
}

RULE_FILTER=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rule) RULE_FILTER="$2"; shift 2 ;;
    *) shift ;;
  esac
done

log "Alert runner started (filter=${RULE_FILTER:-all})"

for rule in "$ALERT_DIR"/*.yml; do
  [[ -f "$rule" ]] || continue
  name=$(grep '^name:' "$rule" | head -1 | sed 's/name: *//')
  if [[ -n "$RULE_FILTER" ]]; then
    [[ "$name" == "$RULE_FILTER" ]] || continue
  fi
  # Skip manual-only rules unless explicitly requested via --rule
  schedule=$(grep '^schedule:' "$rule" | head -1 | sed 's/schedule: *//' | sed 's/ *#.*//')
  if [[ "$schedule" == "manual" ]] && [[ -z "$RULE_FILTER" ]]; then
    log "SKIP $name — manual schedule (use --rule $name to run)"
    continue
  fi
  run_check "$rule"
done

log "Alert runner complete"
