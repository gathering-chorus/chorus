#!/bin/bash

# app-state.sh — Unified application and infrastructure state management
# Native LaunchAgent architecture. (ADR-019, #2075)
#
# Usage: ./app-state.sh [command] [options]
#
# All services run as native LaunchAgents. No container runtime required.

set -e

# ============================================================================
# CONFIGURATION
# ============================================================================

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." &> /dev/null && pwd )"
LOGS_DIR="$PROJECT_ROOT/logs"
SUPPRESS_FILE="/tmp/chorus-alert-suppress"

# LaunchAgent labels
APP_LABEL="com.gathering.app"
FUSEKI_LABEL="com.gathering.fuseki"
PROMETHEUS_LABEL="com.gathering.prometheus"
GRAFANA_LABEL="com.gathering.grafana"
LOKI_LABEL="com.gathering.loki"
PROMTAIL_LABEL="com.gathering.promtail"

# Load environment variables from .env if it exists
if [ -f "$PROJECT_ROOT/.env" ]; then
  set -a
  source "$PROJECT_ROOT/.env"
  set +a
fi

# ============================================================================
# LOGGING
# ============================================================================

log() {
  local level="$1"
  local message="$2"
  local timestamp=$(TZ=America/New_York date +"%Y-%m-%d %H:%M:%S")
  echo "{\"timestamp\":\"$timestamp\",\"level\":\"$level\",\"appName\":\"jeff-bridwell-personal-site\",\"message\":\"$message\"}"
}

# Shared observability project location
OBSERVABILITY_DIR="$(dirname "$PROJECT_ROOT")/shared-observability"

# ============================================================================
# ALERT SUPPRESSION (#2305)
# ============================================================================

write_suppress() {
  local ttl="${1:-120}"
  local expiry=$(( $(date +%s) + ttl ))
  echo "$expiry" > "$SUPPRESS_FILE"
  log "info" "Alert suppression active for ${ttl}s (expires $(date -r "$expiry" '+%H:%M:%S'))"
}

cmd_suppress() {
  local ttl="${1:-120}"
  write_suppress "$ttl"
}

# ============================================================================
# LAUNCHAGENT HELPERS
# ============================================================================

uid() { id -u; }

service_running() {
  local label="$1"
  launchctl print "gui/$(uid)/$label" 2>/dev/null | grep -q "state = running"
}

service_start() {
  local label="$1"
  launchctl kickstart "gui/$(uid)/$label" 2>/dev/null || {
    launchctl bootstrap "gui/$(uid)" ~/Library/LaunchAgents/${label}.plist 2>/dev/null
    launchctl kickstart "gui/$(uid)/$label" 2>/dev/null
  }
}

service_stop() {
  local label="$1"
  launchctl kill SIGTERM "gui/$(uid)/$label" 2>/dev/null || true
}

service_restart() {
  local label="$1"
  service_stop "$label"
  sleep 2
  service_start "$label"
}

# ============================================================================
# HEALTH CHECKS
# ============================================================================

wait_for_health() {
  local max_attempts=${1:-30}
  local attempt=0

  log "info" "Waiting for app to become healthy..."

  while [ $attempt -lt $max_attempts ]; do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:3000 2>/dev/null | grep -q "200\|302"; then
      log "info" "App is healthy and responding"
      return 0
    fi
    ((attempt++))
    sleep 1
  done

  log "warn" "Health check timed out after $max_attempts attempts"
  return 1
}

check_health() {
  local all_healthy=true

  log "info" "Checking infrastructure health..."

  # Check app LaunchAgent
  if service_running "$APP_LABEL"; then
    log "info" "App ($APP_LABEL): RUNNING"
  else
    log "warn" "App ($APP_LABEL): NOT RUNNING"
    all_healthy=false
  fi

  # Check app endpoint
  if curl -s -o /dev/null http://localhost:3000 2>/dev/null; then
    log "info" "Express app (port 3000): RESPONDING"
  else
    log "warn" "Express app (port 3000): NOT RESPONDING"
    all_healthy=false
  fi

  # Check Fuseki
  local fuseki_url="${FUSEKI_URL:-http://localhost:3030}"
  if service_running "$FUSEKI_LABEL"; then
    log "info" "Fuseki ($FUSEKI_LABEL): RUNNING"
    if curl -s -o /dev/null "$fuseki_url/\$/ping" 2>/dev/null; then
      log "info" "Fuseki ($fuseki_url): RESPONDING"
    else
      log "warn" "Fuseki ($fuseki_url): NOT RESPONDING"
    fi
  else
    log "warn" "Fuseki ($FUSEKI_LABEL): NOT RUNNING"
  fi

  # Check observability services
  if service_running "$PROMETHEUS_LABEL"; then
    log "info" "Prometheus ($PROMETHEUS_LABEL): RUNNING"
    if curl -s -o /dev/null http://localhost:9090 2>/dev/null; then
      log "info" "Prometheus (port 9090): RESPONDING"
    else
      log "warn" "Prometheus (port 9090): NOT RESPONDING"
    fi
  else
    log "warn" "Prometheus ($PROMETHEUS_LABEL): NOT RUNNING"
  fi

  if service_running "$GRAFANA_LABEL"; then
    log "info" "Grafana ($GRAFANA_LABEL): RUNNING"
    if curl -s -o /dev/null http://localhost:3100 2>/dev/null; then
      log "info" "Grafana (port 3100): RESPONDING"
    else
      log "warn" "Grafana (port 3100): NOT RESPONDING"
    fi
  else
    log "warn" "Grafana ($GRAFANA_LABEL): NOT RUNNING"
  fi

  if service_running "$LOKI_LABEL"; then
    log "info" "Loki ($LOKI_LABEL): RUNNING"
    if curl -s -o /dev/null http://localhost:3102/ready 2>/dev/null; then
      log "info" "Loki (port 3102): RESPONDING"
    else
      log "warn" "Loki (port 3102): NOT RESPONDING"
    fi
  else
    log "warn" "Loki ($LOKI_LABEL): NOT RUNNING"
  fi

  if service_running "$PROMTAIL_LABEL"; then
    log "info" "Promtail ($PROMTAIL_LABEL): RUNNING"
  else
    log "warn" "Promtail ($PROMTAIL_LABEL): NOT RUNNING (logs not being shipped to Loki)"
  fi

  # Check Cloudflare tunnel
  local tunnel_pid_file="$PROJECT_ROOT/.cloudflared/tunnel.pid"
  if [ -f "$tunnel_pid_file" ]; then
    local tunnel_pid=$(cat "$tunnel_pid_file")
    if ps -p "$tunnel_pid" > /dev/null 2>&1; then
      local tunnel_host=""
      if [ -f "$PROJECT_ROOT/.cloudflared/.env.tunnel" ]; then
        tunnel_host=$(grep "^TUNNEL_HOSTNAME=" "$PROJECT_ROOT/.cloudflared/.env.tunnel" 2>/dev/null | cut -d= -f2)
      fi
      log "info" "Cloudflare tunnel (PID $tunnel_pid): RUNNING${tunnel_host:+ → https://$tunnel_host}"
    else
      log "warn" "Cloudflare tunnel: NOT RUNNING (stale PID file)"
    fi
  else
    log "info" "Cloudflare tunnel: NOT CONFIGURED (optional — see scripts/tunnel/)"
  fi

  if $all_healthy; then
    return 0
  else
    return 1
  fi
}

# ============================================================================
# TESTING
# ============================================================================

run_tests() {
  log "info" "Running test suite..."
  cd "$PROJECT_ROOT"

  log "info" "Running unit tests..."
  npm run test:unit || { log "error" "Unit tests failed"; return 1; }

  log "info" "Running integration tests..."
  npm run test:integration || { log "error" "Integration tests failed"; return 1; }

  log "info" "Running security tests..."
  npm run test:security || { log "error" "Security tests failed"; return 1; }

  log "info" "Running performance tests..."
  npm run test:performance || { log "error" "Performance tests failed"; return 1; }

  log "info" "All tests passed"
  return 0
}

# ============================================================================
# COMMANDS
# ============================================================================

cmd_start() {
  local run_tests=false

  for arg in "$@"; do
    case $arg in
      --test|--with-tests)
        run_tests=true
        ;;
    esac
  done

  if $run_tests; then
    run_tests || exit 1
  fi

  # Start Fuseki if not running
  if ! service_running "$FUSEKI_LABEL"; then
    log "info" "Starting Fuseki..."
    service_start "$FUSEKI_LABEL"
    local i=0
    while [ $i -lt 30 ]; do
      if curl -sf --max-time 2 "http://localhost:3030/\$/ping" &>/dev/null; then
        log "info" "Fuseki started — http://localhost:3030"
        break
      fi
      sleep 2; ((i+=2))
    done
    if [ $i -ge 30 ]; then
      log "warn" "Fuseki started but not healthy after 30s"
    fi
  else
    log "info" "Fuseki already running"
  fi

  # Start app if not running
  if service_running "$APP_LABEL"; then
    log "info" "App already running"
  else
    log "info" "Starting app..."
    service_start "$APP_LABEL"
  fi

  wait_for_health
  log "info" "App available at http://localhost:3000"
}

cmd_stop() {
  write_suppress 120

  local stopped=false

  if service_running "$APP_LABEL"; then
    log "info" "Stopping app..."
    service_stop "$APP_LABEL"
    stopped=true
  fi

  if service_running "$FUSEKI_LABEL"; then
    log "info" "Stopping Fuseki..."
    service_stop "$FUSEKI_LABEL"
    stopped=true
  fi

  if $stopped; then
    log "info" "Services stopped"
  else
    log "info" "Services already stopped"
  fi
}

cmd_restart() {
  local run_tests=false

  for arg in "$@"; do
    case $arg in
      --test|--with-tests)
        run_tests=true
        ;;
    esac
  done

  if $run_tests; then
    run_tests || exit 1
  fi

  write_suppress 120

  log "info" "Restarting app via launchctl..."
  service_restart "$APP_LABEL"

  wait_for_health
  log "info" "Restart complete — app available at http://localhost:3000"
}

cmd_status() {
  check_health
}

cmd_logs() {
  local query='{appName="jeff-bridwell-personal-site"}'
  local loki_url="http://localhost:3102"
  local grafana_url="http://localhost:3100"

  # Parse options
  for arg in "$@"; do
    case $arg in
      --errors)
        query='{appName="jeff-bridwell-personal-site"} | json | level="error"'
        ;;
      --warn)
        query='{appName="jeff-bridwell-personal-site"} | json | level="warn"'
        ;;
    esac
  done

  # Check if Loki is reachable
  if curl -s -o /dev/null "$loki_url/ready" 2>/dev/null; then
    log "info" "Querying Loki for recent logs..."
    local result
    result=$(curl -s -G "$loki_url/loki/api/v1/query_range" \
      --data-urlencode "query=$query" \
      --data-urlencode "limit=50" \
      --data-urlencode "start=$(date -u -v-1H +%s 2>/dev/null || date -u -d '1 hour ago' +%s)000000000" \
      --data-urlencode "end=$(date -u +%s)000000000" 2>/dev/null)

    if [ -n "$result" ]; then
      echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    results = d.get('data', {}).get('result', [])
    for stream in results:
        for ts, line in stream.get('values', []):
            print(line)
except Exception as e:
    print(f'Error parsing Loki response: {e}', file=sys.stderr)
" 2>/dev/null
    fi

    log "info" "For full log exploration, open Grafana: $grafana_url/explore"
    log "info" "Loki query: $query"
  else
    log "warn" "Loki is not reachable at $loki_url"
    log "info" "Check Loki: launchctl print gui/$(uid)/$LOKI_LABEL"

    # Fallback to local log file
    local app_log="/tmp/gathering-app.log"
    if [ -f "$app_log" ]; then
      log "info" "Falling back to local log file..."
      tail -n 50 "$app_log"
    else
      log "warn" "No local log file at $app_log"
    fi
  fi
}

cmd_test() {
  run_tests
}

cmd_help() {
  cat << 'EOF'
Usage: ./app-state.sh [command] [options]

All services run as native LaunchAgents. No container runtime required.

Commands:
  start [--test]    Start app + Fuseki LaunchAgents
                    --test: Run tests before starting

  stop              Stop app + Fuseki LaunchAgents

  restart [--test]  Restart the app LaunchAgent
                    --test: Run tests before restarting

  status            Check health of all components:
                    App, Fuseki, observability (Prometheus, Grafana, Loki,
                    Promtail), Cloudflare tunnel

  logs [--errors|--warn]
                    Query logs via Loki (falls back to local log file)
                    --errors: Show only error-level logs
                    --warn: Show only warning-level logs

  test              Run full test suite (unit, integration, security, performance)

  suppress [ttl]    Suppress alerts for ttl seconds (default: 120)

  help              Show this help message

Managed services (LaunchAgents):
  com.gathering.app         Express application (port 3000)
  com.gathering.fuseki      SPARQL triplestore (port 3030)

Observability services (checked by status, not managed):
  com.gathering.prometheus  Prometheus (port 9090)
  com.gathering.grafana     Grafana (port 3100)
  com.gathering.loki        Loki (port 3102)
  com.gathering.promtail    Promtail

Examples:
  ./app-state.sh start              # Start app + Fuseki
  ./app-state.sh start --test       # Run tests, then start
  ./app-state.sh restart            # Restart app
  ./app-state.sh stop               # Stop app + Fuseki
  ./app-state.sh logs               # Query recent logs from Loki
  ./app-state.sh logs --errors      # Query error logs from Loki
  ./app-state.sh status             # Full health check
EOF
}

# ============================================================================
# MAIN
# ============================================================================

mkdir -p "$LOGS_DIR"

case "${1:-help}" in
  start)
    shift
    cmd_start "$@"
    ;;
  stop)
    cmd_stop
    ;;
  restart)
    shift
    cmd_restart "$@"
    ;;
  status)
    cmd_status
    ;;
  logs)
    shift
    cmd_logs "$@"
    ;;
  test)
    cmd_test
    ;;
  suppress)
    shift
    cmd_suppress "$@"
    ;;
  help|--help|-h)
    cmd_help
    ;;
  *)
    log "error" "Unknown command: $1"
    cmd_help
    exit 1
    ;;
esac
