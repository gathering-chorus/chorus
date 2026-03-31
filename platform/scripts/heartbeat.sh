#!/bin/bash
# heartbeat.sh — Emit pulse event every run. KeepAlive LaunchAgent calls this every 5 min.
# If this stops, launchd restarts it. If the machine is down, Bedroom health probe catches it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_LOG="${SCRIPT_DIR}/chorus-log.sh"

# Emit heartbeat pulse
bash "$CHORUS_LOG" system.heartbeat silas --level=info

# Quick health checks — emit warn/critical if something is down
check_service() {
  local name="$1" url="$2" level="$3"
  local code
  code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 3 "$url" 2>/dev/null || echo "000")
  if [ "$code" = "000" ] || [ "$code" = "503" ]; then
    bash "$CHORUS_LOG" "system.service.down" silas "service=${name},code=${code}" --level="$level"
  fi
}

check_service "app" "http://localhost:3000/health" "critical"
check_service "fuseki" "http://localhost:3030/$/ping" "warn"
check_service "clearing" "http://localhost:3470/" "warn"
check_service "chorus-api" "http://localhost:3340/" "warn"
check_service "vikunja" "http://localhost:3456/" "warn"
