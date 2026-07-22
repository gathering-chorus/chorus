#!/usr/bin/env bash
# test-product-membrane.sh — #3611 UNTANGLE AC2: the factory↔product membrane test.
#
# Jeff's invariant #6: gathering is a product with ZERO runtime dependency on
# chorus. This proves it against the RUNNING system: stop every com.chorus.*
# LaunchAgent, probe gathering's serving surface, restart chorus, report.
#
# Run as ONE invocation — while chorus is down the hook daemon is down too, so
# nothing else (no other tool calls, no gates) should run in the window. The
# script restarts chorus in a trap: an assertion failure can never strand the
# chorus stack stopped.
#
# Usage: test-product-membrane.sh [--dry-run]
#   --dry-run  list what would be stopped/probed, touch nothing
set -uo pipefail

UID_N="$(id -u)"
RESULTS="${MEMBRANE_RESULTS:-/tmp/membrane-test-$(date +%Y%m%d-%H%M%S).txt}"

# Every loaded com.chorus.* agent with a live PID (running services only —
# periodic jobs without a PID have nothing to stop).
chorus_running() {
  launchctl list | awk '$1 ~ /^[0-9]+$/ && $3 ~ /^com\.chorus\./ {print $3}'
}

# Gathering's serving surface, probed as a USER of the product (HTTP, not ps).
# name|url|expect — expect is a grep -E pattern on the HTTP code.
PROBES=(
  "app-health|http://localhost:3000/health|200"
  "app-page|http://localhost:3000/|200|30[12]"
  "fuseki-ping|http://localhost:3030/\$/ping|200"
  "fuseki-read|http://localhost:3030/pods/sparql?query=ASK%7B%7D|200"
)

probe_gathering() {
  local phase="$1" all_ok=0
  for p in "${PROBES[@]}"; do
    IFS='|' read -r name url e1 e2 <<<"$p"
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 8 "$url" 2>/dev/null || echo "000")
    local ok="FAIL"
    if [[ "$code" =~ ^(${e1}${e2:+|$e2})$ ]]; then ok="ok"; else all_ok=1; fi
    echo "[$phase] $name $url -> $code $ok" | tee -a "$RESULTS"
  done
  return $all_ok
}

STOPPED=()
restore_chorus() {
  for label in "${STOPPED[@]+"${STOPPED[@]}"}"; do
    launchctl bootstrap "gui/$UID_N" "$HOME/Library/LaunchAgents/$label.plist" 2>/dev/null \
      || launchctl kickstart "gui/$UID_N/$label" 2>/dev/null || true
  done
  echo "[restore] chorus services restarted: ${#STOPPED[@]}" | tee -a "$RESULTS"
}

main() {
  local services
  services=$(chorus_running)
  echo "membrane test $(date '+%Y-%m-%d %H:%M:%S') — chorus running services:" | tee "$RESULTS"
  echo "$services" | tee -a "$RESULTS"

  if [[ "${1:-}" == "--dry-run" ]]; then
    echo "[dry-run] would stop the above, probe gathering, restart" | tee -a "$RESULTS"
    probe_gathering "baseline" || true
    exit 0
  fi

  # Baseline: gathering must serve BEFORE the test or the test proves nothing.
  if ! probe_gathering "baseline"; then
    echo "BASELINE FAIL — gathering not fully serving with chorus UP; aborting (nothing stopped)" | tee -a "$RESULTS"
    exit 2
  fi

  trap restore_chorus EXIT

  while IFS= read -r label; do
    [ -n "$label" ] || continue
    if launchctl bootout "gui/$UID_N/$label" 2>/dev/null; then
      STOPPED+=("$label")
    fi
  done <<<"$services"
  echo "[stop] chorus services stopped: ${#STOPPED[@]}" | tee -a "$RESULTS"
  sleep 2

  local remaining
  remaining=$(chorus_running | wc -l | tr -d ' ')
  echo "[stop] com.chorus.* still running: $remaining" | tee -a "$RESULTS"

  if probe_gathering "chorus-down"; then
    echo "MEMBRANE OK — gathering serves with chorus fully stopped" | tee -a "$RESULTS"
    rc=0
  else
    echo "MEMBRANE FAIL — a gathering surface degraded while chorus was down" | tee -a "$RESULTS"
    rc=1
  fi

  restore_chorus
  trap - EXIT
  sleep 2
  echo "[verify] com.chorus.* running after restore: $(chorus_running | wc -l | tr -d ' ')" | tee -a "$RESULTS"
  echo "results: $RESULTS"
  exit "$rc"
}

main "${1:-}"
