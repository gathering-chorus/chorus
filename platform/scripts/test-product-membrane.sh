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

# #3722 — SELF-GUARD: this test bootouts every com.chorus.* agent. If it is
# ITSELF running under one (e.g. the nightly-suites LaunchAgent invoked it), it
# would kill its own runner mid-loop — the ~13-min "untrappable" nightly killer
# (Jul 22–Aug 2). Refuse in that case; this belongs to an ops/CI run with full
# restore authority, not inside an agent it will stop. Walk the ancestry via ps.
# #4004 — the ancestry scan below is a NAME LIST, and name lists drift. #3974
# moved suite execution into the werk-test binary, so the parent command became
# `werk-test` and matched neither pattern. Log evidence from the 17:44 run: this
# suite scored "verdict fail — 0 pass, 1 fail" and NOT the SELF-REFUSED rc=3 line
# a firing guard produces, while the same run's api row read "1862 pass, 246
# fail" — the bootout-collateral signature this card is named after. #3722's
# guard was defeated not by a hole in its logic but by a rename underneath it.
# Kade's ask, and he is right: match something that cannot be renamed.
#
# A controlling terminal is that invariant. An ops run has one; every automated
# runner — werk-test, launchd, act, cron — does not, and no future rename changes
# that. The name scan stays underneath as a second net.
if [ "${MEMBRANE_ALLOW_UNDER_AGENT:-0}" != "1" ] && [ ! -t 0 ]; then
  echo "REFUSED — test-product-membrane has no controlling terminal, so it is running under an automated runner; it would bootout that runner and every other com.chorus.* agent. Run it from an ops shell, or set MEMBRANE_ALLOW_UNDER_AGENT=1 if you own the restore. (#4004)" >&2
  exit 3
fi

if [ "${MEMBRANE_ALLOW_UNDER_AGENT:-0}" != "1" ]; then
  _pid=$PPID
  while [ "${_pid:-0}" -gt 1 ]; do
    _cmd="$(ps -o command= -p "$_pid" 2>/dev/null || true)"
    case "$_cmd" in
      *com.chorus.*|*nightly-suites.sh*)
        echo "REFUSED — test-product-membrane runs under a chorus agent ancestor (pid $_pid: ${_cmd%% *}); it would bootout its own runner. Run from an ops shell, or set MEMBRANE_ALLOW_UNDER_AGENT=1 if you own the restore. (#3722)" >&2
        exit 3 ;;
    esac
    _pid="$(ps -o ppid= -p "$_pid" 2>/dev/null | tr -d ' ')"
  done
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
