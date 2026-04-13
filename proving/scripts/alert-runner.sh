#!/usr/bin/env bash
# alert-runner.sh — Execute all YAML alert checks and fire actions on failure
# Card #2000 | Silas
#
# Reads alert rules from chorus/alerting/*.yml, runs each check script,
# fires the action script if check fails.
#
# Usage: alert-runner.sh [--rule <name>]

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

ALERT_DIR="${CHORUS_ROOT}/proving/domains/alerts"
ALERT_STATE_DIR="/Users/jeffbridwell/Library/Logs/Gathering/alert-state"
LOG="/Users/jeffbridwell/Library/Logs/Chorus/alert-runner.log"
COOLDOWN_SECONDS="${ALERT_COOLDOWN:-600}"  # 10 minutes default
CONSECUTIVE_THRESHOLD="${ALERT_THRESHOLD:-2}"  # require 2 failures before firing
TIMESTAMP() { TZ=America/New_York date '+%Y-%m-%d %H:%M:%S'; }

mkdir -p "$(dirname "$LOG")" "$ALERT_STATE_DIR"

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
    # Reset consecutive failure count on success
    rm -f "$ALERT_STATE_DIR/${name}.consecutive"
    return
  fi

  # Track consecutive failures
  local consec_file="$ALERT_STATE_DIR/${name}.consecutive"
  local fail_count=0
  [[ -f "$consec_file" ]] && fail_count=$(cat "$consec_file")
  fail_count=$((fail_count + 1))
  echo "$fail_count" > "$consec_file"

  # Require consecutive failures before firing
  if [[ $fail_count -lt $CONSECUTIVE_THRESHOLD ]]; then
    log "WARN $name — failure $fail_count/$CONSECUTIVE_THRESHOLD, waiting for consecutive threshold"
    return
  fi

  # Cooldown — skip if fired too recently
  local fire_file="$ALERT_STATE_DIR/${name}.last_fire"
  if [[ -f "$fire_file" ]]; then
    local last_fire now elapsed
    last_fire=$(cat "$fire_file")
    now=$(date +%s)
    elapsed=$((now - last_fire))
    if [[ $elapsed -lt $COOLDOWN_SECONDS ]]; then
      log "SKIP $name — cooldown active (${elapsed}s/${COOLDOWN_SECONDS}s since last fire)"
      return
    fi
  fi

  log "FIRE $name — $result (consecutive: $fail_count)"

  local action_script
  action_script=$(awk '/^action: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")

  if [[ -n "$action_script" ]]; then
    bash -c "$action_script" >> "$LOG" 2>&1 || true
    date +%s > "$fire_file"
    log "  ACTION $name fired — cooldown ${COOLDOWN_SECONDS}s started"
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
