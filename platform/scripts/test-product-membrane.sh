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

# #4004 — this suite bootouts EVERY com.chorus.* agent, so the question is not
# "who is my parent" but "does this run hold restore authority". Inference kept
# failing: #3722 scanned ancestry for com.chorus. / nightly-suites.sh, #3974
# renamed the runner to werk-test and the scan went quiet — the nightly booted
# every agent and platform/api took 246 collateral failures. The obvious repair,
# refusing without a controlling terminal, is ALSO wrong: act allocates a pty, so
# a pipeline run looks exactly like an operator sitting at a keyboard (proven by
# this card's own test failing inside the pipeline while passing locally).
#
# So stop inferring. Running this requires an EXPLICIT grant. Unattended runs
# self-refuse with rc=3, which the nightly scores as SELF-REFUSED rather than a
# failure; an operator who owns the restore sets the grant and runs it. A guard
# that must be granted cannot be defeated by a rename or a pty.
if [ "${MEMBRANE_ALLOW_UNDER_AGENT:-0}" != "1" ]; then
  echo "REFUSED — test-product-membrane bootouts every com.chorus.* agent and needs explicit restore authority. It never runs unattended: set MEMBRANE_ALLOW_UNDER_AGENT=1 from an ops shell where you own the restore. (#4004)" >&2
  exit 3
fi

UID_N="$(id -u)"
RESULTS="${MEMBRANE_RESULTS:-/tmp/membrane-test-$(date +%Y%m%d-%H%M%S).txt}"

# Every loaded com.chorus.* agent with a live PID (running services only —
# periodic jobs without a PID have nothing to stop).
chorus_running() {
  # #3722 — never list com.chorus.session-watcher/nightly-suites as stoppable if
  # this run somehow descends from one (defense-in-depth behind the self-guard).
  launchctl list | awk '$1 ~ /^[0-9]+$/ && $3 ~ /^com\.chorus\./ {print $3}'
}

# Gathering's serving surface, probed as a USER of the product (HTTP, not ps).
# name|url|expect — expect is a grep -E pattern on the HTTP code.
# 2026-07-22 run finding: :3000 is chorus's caddy edge (#2122) proxying the app's
# real home :3002 — the front-door probe and the direct-app probe are SEPARATE
# rows so the test distinguishes "product process dead" from "front door dead".
PROBES=(
  "frontdoor-health|http://localhost:3000/health|200"
  "frontdoor-page|http://localhost:3000/|200|30[12]"
  "app-direct|http://localhost:3002/health|200"
  "fuseki-ping|http://localhost:3030/\$/ping|200"
  "fuseki-read|http://localhost:3030/pods/sparql?query=ASK%7B%7D|200"
)

probe_gathering() {
  local phase="$1" all_ok=0
  for p in "${PROBES[@]}"; do
    IFS='|' read -r name url e1 e2 <<<"$p"
    local code
    # -w prints its own 000 on connect failure — no || fallback, or codes double up.
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 8 "$url" 2>/dev/null) || true
    local ok="FAIL"
    if [[ "$code" =~ ^(${e1}${e2:+|$e2})$ ]]; then ok="ok"; else all_ok=1; fi
    echo "[$phase] $name $url -> $code $ok" | tee -a "$RESULTS"
  done
  return $all_ok
}

STOPPED=()
restore_chorus() {
  # 2026-07-22 run finding: bootstrap right after bootout can fail transiently
  # (launchd I/O error) — the first run left com.chorus.hooks unloaded, which is
  # a TEAM-WIDE fail-closed lockout (#2790). Retry each bootstrap up to 3x and
  # report any service that still refused; kickstart is useless after bootout
  # (the label no longer exists in the domain), so it is not a fallback.
  local failed=()
  for label in "${STOPPED[@]+"${STOPPED[@]}"}"; do
    local ok=1
    for _try in 1 2 3; do
      if launchctl bootstrap "gui/$UID_N" "$HOME/Library/LaunchAgents/$label.plist" 2>/dev/null; then
        ok=0; break
      fi
      sleep 1
    done
    [ "$ok" -eq 0 ] || failed+=("$label")
  done
  echo "[restore] chorus services restarted: $(( ${#STOPPED[@]} - ${#failed[@]} ))/${#STOPPED[@]}" | tee -a "$RESULTS"
  if [ "${#failed[@]}" -gt 0 ]; then
    echo "[restore] STILL DOWN — fix by hand NOW (hooks down = team lockout): ${failed[*]}" | tee -a "$RESULTS"
  fi
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
