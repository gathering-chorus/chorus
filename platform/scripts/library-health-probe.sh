#!/bin/bash
# library-health-probe.sh — Runs on Bedroom, checks Library services via SSH/HTTP.
# Second watchdog layer: if Library is down or services crashed, alert.
# Scheduled hourly on Bedroom via cron or LaunchAgent.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

LIBRARY="192.168.86.36"
ALERT_TARGET="jeff"

check_http() {
  local name="$1" port="$2" path="$3"
  local code
  code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "http://${LIBRARY}:${port}${path}" 2>/dev/null || echo "000")
  if [ "$code" = "000" ]; then
    echo "DOWN: ${name} (port ${port}) — connection refused"
    return 1
  elif [ "$code" = "503" ]; then
    echo "DEGRADED: ${name} (port ${port}) — 503"
    return 1
  else
    echo "OK: ${name} (port ${port}) — ${code}"
    return 0
  fi
}

echo "=== Library Health Probe ==="
echo "$(date '+%Y-%m-%d %H:%M') from Bedroom"
echo ""

FAILURES=0

check_http "App" 3000 "/health" || ((FAILURES++))
check_http "Fuseki" 3030 "/$/ping" || ((FAILURES++))
check_http "Clearing" 3470 "/" || ((FAILURES++))
check_http "Chorus API" 3340 "/" || ((FAILURES++))
check_http "Vikunja" 3456 "/" || ((FAILURES++))

echo ""
if [ "$FAILURES" -gt 0 ]; then
  echo "ALERT: ${FAILURES} service(s) down on Library"
  # SSH back to Library to emit critical nudge to Jeff
  ssh "${LIBRARY}" "bash ${CHORUS_ROOT}/platform/scripts/nudge jeff '[critical] Bedroom probe: ${FAILURES} service(s) down on Library' --level critical --from system" 2>/dev/null || true
else
  echo "All services healthy"
fi
