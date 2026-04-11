#!/bin/bash

# app-state.sh - Unified application and infrastructure state management
# Usage: ./app-state.sh [command] [options]
#
# This script manages both infrastructure (Docker containers, networking) and
# application state (starting, stopping, testing) in a single unified interface.

set -e

# ============================================================================
# CONFIGURATION
# ============================================================================

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." &> /dev/null && pwd )"
TERRAFORM_DIR="$PROJECT_ROOT/terraform/environments/dev"
LOGS_DIR="$PROJECT_ROOT/logs"
APP_CONTAINER="jeff-bridwell-personal-site-app"
FUSEKI_CONTAINER="jeff-bridwell-personal-site-fuseki"
NETWORK_NAME="jeff-bridwell-personal-site-network"
SUPPRESS_FILE="/tmp/chorus-alert-suppress"

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
# PREREQUISITES
# ============================================================================

check_docker() {
  if ! command -v docker &> /dev/null; then
    log "error" "Docker is not installed. Please install Docker first."
    exit 1
  fi

  if ! docker info &>/dev/null; then
    log "info" "Docker daemon is not running. Attempting to start..."
    if [[ "$(uname)" == "Darwin" ]]; then
      open -a Docker
      local attempts=0
      while ! docker info &>/dev/null && [ $attempts -lt 30 ]; do
        sleep 1
        ((attempts++))
      done
    fi

    if ! docker info &>/dev/null; then
      log "error" "Could not start Docker. Please start Docker manually."
      exit 1
    fi
    log "info" "Docker started successfully"
  fi
}

check_observability() {
  # Check if observability network exists
  if ! docker network ls --format '{{.Name}}' | grep -q "^observability-network$"; then
    log "warn" "Shared observability network not found"

    # Check if observability project exists
    if [ -d "$OBSERVABILITY_DIR" ] && [ -f "$OBSERVABILITY_DIR/scripts/observability.sh" ]; then
      log "info" "Starting shared observability stack..."
      "$OBSERVABILITY_DIR/scripts/observability.sh" start
    else
      log "warn" "Shared observability project not found at $OBSERVABILITY_DIR"
      log "info" "Creating observability-network manually (logs/metrics may not be collected)"
      docker network create observability-network 2>/dev/null || true
    fi
  else
    log "info" "Shared observability network: OK"
  fi
}

check_terraform() {
  if ! command -v terraform &> /dev/null; then
    log "error" "Terraform is not installed. Please install Terraform first."
    exit 1
  fi
}

# ============================================================================
# INFRASTRUCTURE MANAGEMENT
# ============================================================================

fuseki_running() {
  docker ps --filter "name=$FUSEKI_CONTAINER" --filter "status=running" -q | grep -q .
}

init_fuseki() {
  # Skip if Fuseki isn't running
  if ! fuseki_running; then
    log "info" "Fuseki not running, skipping dataset initialization"
    return 0
  fi

  local FUSEKI_URL="${FUSEKI_URL:-http://localhost:3030}"
  local FUSEKI_DATASET="${FUSEKI_DATASET:-pods}"
  local FUSEKI_ADMIN_PASSWORD="${FUSEKI_ADMIN_PASSWORD:-admin123}"

  log "info" "Waiting for Fuseki to be ready..."

  # Wait for Fuseki to be healthy
  local attempts=0
  local max_attempts=30
  while [ $attempts -lt $max_attempts ]; do
    if curl -s -o /dev/null "$FUSEKI_URL/\$/ping" 2>/dev/null; then
      break
    fi
    ((attempts++))
    sleep 2
  done

  if [ $attempts -eq $max_attempts ]; then
    log "warn" "Fuseki did not become ready, skipping dataset initialization"
    return 0
  fi

  # Check if dataset exists
  if curl -s -u "admin:$FUSEKI_ADMIN_PASSWORD" "$FUSEKI_URL/\$/datasets" 2>/dev/null | grep -q "\"/$FUSEKI_DATASET\""; then
    log "info" "Fuseki dataset '$FUSEKI_DATASET' already exists"
    return 0
  fi

  # Create dataset
  log "info" "Creating Fuseki dataset '$FUSEKI_DATASET'..."
  local response
  response=$(curl -s -w "%{http_code}" -X POST \
    -u "admin:$FUSEKI_ADMIN_PASSWORD" \
    -H "Content-Type: application/x-www-form-urlencoded" \
    -d "dbName=$FUSEKI_DATASET&dbType=tdb2" \
    "$FUSEKI_URL/\$/datasets" 2>/dev/null)

  local http_code="${response: -3}"
  if [ "$http_code" = "200" ] || [ "$http_code" = "201" ]; then
    log "info" "Fuseki dataset '$FUSEKI_DATASET' created successfully"
  else
    log "warn" "Failed to create Fuseki dataset (HTTP $http_code), continuing..."
  fi
}

infra_exists() {
  docker ps -a --filter "name=$APP_CONTAINER" --format "{{.Names}}" | grep -q "$APP_CONTAINER"
}

infra_running() {
  docker ps --filter "name=$APP_CONTAINER" --filter "status=running" --format "{{.Names}}" | grep -q "$APP_CONTAINER"
}

create_infra() {
  log "info" "Creating infrastructure with Terraform..."

  mkdir -p "$LOGS_DIR"

  cd "$TERRAFORM_DIR"

  if [ ! -d ".terraform" ]; then
    log "info" "Initializing Terraform..."
    terraform init -input=false
  fi

  log "info" "Applying Terraform configuration..."
  terraform apply -auto-approve

  if [ $? -eq 0 ]; then
    log "info" "Infrastructure created successfully"

    # NOTE: Monitoring is provided by the shared-observability project
    # The old embedded prometheus/ stack was removed 2026-02-22 (card #127)

    # Initialize Fuseki dataset if Fuseki is running
    init_fuseki

    return 0
  else
    log "error" "Failed to create infrastructure"
    return 1
  fi
}

destroy_infra() {
  log "info" "Destroying infrastructure..."

  # NOTE: Monitoring is now provided by the shared-observability project
  # The embedded stack in terraform/environments/dev/prometheus is deprecated
  # To destroy it manually: cd terraform/environments/dev/prometheus && terraform destroy

  # Destroy main infrastructure
  cd "$TERRAFORM_DIR"
  if [ -f "terraform.tfstate" ]; then
    terraform destroy -auto-approve
  fi

  # Clean up any lingering containers
  local lingering=$(docker ps -a --filter "name=jeff-bridwell-personal-site" -q)
  if [ -n "$lingering" ]; then
    log "info" "Removing lingering containers..."
    docker rm -f $lingering 2>/dev/null || true
  fi

  # Clean up Fuseki data volume
  local fuseki_volume="jeff-bridwell-personal-site-fuseki-data"
  if docker volume ls -q | grep -q "^${fuseki_volume}$"; then
    log "info" "Removing Fuseki data volume..."
    docker volume rm "$fuseki_volume" 2>/dev/null || true
  fi

  # Clean up network if exists
  docker network rm "$NETWORK_NAME" 2>/dev/null || true

  log "info" "Infrastructure destroyed"
}

# ============================================================================
# APPLICATION MANAGEMENT
# ============================================================================

# NOTE: nodemon runs inside the Docker container (via npm run dev).
# No local nodemon is needed — the app container handles it.

restart_container() {
  log "info" "Restarting app container..."
  docker restart "$APP_CONTAINER"
  log "info" "Container restarted"
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

  # Check Docker
  if docker info &>/dev/null; then
    log "info" "Docker: OK"
  else
    log "error" "Docker: NOT RUNNING"
    all_healthy=false
  fi

  # Check containers
  if infra_running; then
    log "info" "App container: RUNNING"
  else
    log "warn" "App container: NOT RUNNING"
    all_healthy=false
  fi

  # Check endpoints
  if curl -s -o /dev/null http://localhost:3000 2>/dev/null; then
    log "info" "Express app (port 3000): RESPONDING"
  else
    log "warn" "Express app (port 3000): NOT RESPONDING"
    all_healthy=false
  fi

  # Check Fuseki
  local fuseki_url="${FUSEKI_URL:-http://localhost:3030}"
  if docker ps --filter "name=$FUSEKI_CONTAINER" --filter "status=running" -q | grep -q .; then
    log "info" "Fuseki container: RUNNING"
    if curl -s -o /dev/null "$fuseki_url/\$/ping" 2>/dev/null; then
      log "info" "Fuseki ($fuseki_url): RESPONDING"
    else
      log "warn" "Fuseki ($fuseki_url): NOT RESPONDING"
    fi
  else
    log "warn" "Fuseki container: NOT RUNNING"
  fi

  # Check shared observability stack
  if docker network ls --format '{{.Name}}' | grep -q "^observability-network$"; then
    log "info" "Observability network: CONNECTED"
    if curl -s -o /dev/null http://localhost:9090 2>/dev/null; then
      log "info" "Prometheus (port 9090): RESPONDING"
    else
      log "warn" "Prometheus (port 9090): NOT RESPONDING (start shared-observability)"
    fi
    if curl -s -o /dev/null http://localhost:3100 2>/dev/null; then
      log "info" "Grafana (port 3100): RESPONDING"
    else
      log "warn" "Grafana (port 3100): NOT RESPONDING (start shared-observability)"
    fi
    if curl -s -o /dev/null http://localhost:3102/ready 2>/dev/null; then
      log "info" "Loki (port 3102): RESPONDING"
    else
      log "warn" "Loki (port 3102): NOT RESPONDING (start shared-observability)"
    fi
    if docker ps --filter "name=promtail" --filter "status=running" -q | grep -q .; then
      log "info" "Promtail: RUNNING"
    else
      log "warn" "Promtail: NOT RUNNING (logs not being shipped to Loki)"
    fi
  else
    log "warn" "Observability network: NOT CONNECTED (run: cd ../shared-observability && ./scripts/observability.sh start)"
  fi

  # Check Cloudflare tunnel
  local tunnel_script="$SCRIPT_DIR/tunnel/tunnel.sh"
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

  # Parse options
  for arg in "$@"; do
    case $arg in
      --test|--with-tests)
        run_tests=true
        ;;
    esac
  done

  check_docker
  check_terraform
  check_observability

  # Run tests if requested
  if $run_tests; then
    run_tests || exit 1
  fi

  # Check if infrastructure exists
  if infra_exists; then
    if infra_running; then
      log "info" "Infrastructure already running"
      # Ensure Fuseki is also running
      for container in "$FUSEKI_CONTAINER"; do
        if docker ps -a --filter "name=$container" -q | grep -q . && \
           ! docker ps --filter "name=$container" --filter "status=running" -q | grep -q .; then
          log "info" "Starting stopped container: $container"
          docker start "$container" 2>/dev/null || true
        fi
      done
      wait_for_health
    else
      log "info" "Infrastructure exists but not running, starting containers..."
      for container in "$APP_CONTAINER" "$FUSEKI_CONTAINER"; do
        if docker ps -a --filter "name=$container" -q | grep -q .; then
          log "info" "Starting $container..."
          docker start "$container" 2>/dev/null || true
        fi
      done
      init_fuseki
      wait_for_health
    fi
  else
    log "info" "Infrastructure does not exist, creating..."
    create_infra
    wait_for_health
  fi

  log "info" "App available at http://localhost:3000"
}

cmd_stop() {
  check_docker
  write_suppress 120

  # Stop all containers (but don't destroy)
  local stopped=false
  for container in "$APP_CONTAINER" "$FUSEKI_CONTAINER"; do
    if docker ps --filter "name=$container" --filter "status=running" -q | grep -q .; then
      log "info" "Stopping $container..."
      docker stop "$container" 2>/dev/null || true
      stopped=true
    fi
  done

  if $stopped; then
    log "info" "All containers stopped"
  else
    log "info" "Containers already stopped"
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

  # Native LaunchAgent restart (preferred path)
  if launchctl list 2>/dev/null | grep -q "com.gathering.app"; then
    write_suppress 120
    log "info" "Restarting gathering-app via launchctl..."
    launchctl kickstart -k "gui/$(id -u)/com.gathering.app"
    wait_for_health
    log "info" "Restart complete - app available at http://localhost:3000"
    return 0
  fi

  # Docker fallback
  check_docker
  if infra_exists && infra_running; then
    log "info" "Performing Docker restart..."
    write_suppress 120
    restart_container
    wait_for_health
    log "info" "Restart complete - app available at http://localhost:3000"
  else
    log "info" "Infrastructure not running, performing full start..."
    cmd_start "$@"
  fi
}

cmd_destroy() {
  check_docker
  check_terraform

  log "info" "This will destroy all infrastructure. Containers and networks will be removed."
  destroy_infra
  log "info" "All infrastructure destroyed"
}

cmd_status() {
  check_docker
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
    log "info" "Start observability: cd ../shared-observability && ./scripts/observability.sh start"
    log "info" "Falling back to docker logs..."
    docker logs --tail=100 "$APP_CONTAINER"
  fi
}

cmd_test() {
  run_tests
}

cmd_help() {
  cat << 'EOF'
Usage: ./app-state.sh [command] [options]

Commands:
  start [--test]    Start all containers (app, Fuseki)
                    Creates infrastructure via Terraform if needed
                    --test: Run tests before starting

  stop              Stop all containers (preserves infrastructure)

  restart [--test]  Smart restart (fast if running, full if not)
                    --test: Run tests before restarting

  destroy           Stop and destroy all infrastructure (containers, volumes, network)

  status            Check health of all components:
                    App, Fuseki, observability (Prometheus, Grafana, Loki,
                    Promtail), Cloudflare tunnel

  logs [--errors|--warn]
                    Query logs via Loki (falls back to docker logs if Loki unavailable)
                    --errors: Show only error-level logs
                    --warn: Show only warning-level logs

  test              Run full test suite (unit, integration, security, performance)

  help              Show this help message

Managed containers:
  jeff-bridwell-personal-site-app       Express application (port 3000)
  jeff-bridwell-personal-site-fuseki    SPARQL triplestore (port 3030)

External dependencies (shared-observability project):
  Prometheus (9090), Grafana (3100), Loki (3102), Promtail

Examples:
  ./app-state.sh start              # Start app, create infra if needed
  ./app-state.sh start --test       # Run tests, then start
  ./app-state.sh restart            # Fast restart
  ./app-state.sh stop               # Stop all containers
  ./app-state.sh destroy            # Full cleanup (containers + volumes)
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
  destroy)
    cmd_destroy
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
