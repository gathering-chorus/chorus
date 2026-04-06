#!/usr/bin/env bash
# deep-health.sh — subprocess liveness, not wrapper alive (#2228)
# Runs on 5-min cron. Alerts via nudge --force on failure.
set -euo pipefail

NUDGE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log"
ALERT_ROLE="silas"
FAILURES=()

now=$(date +%s)

# --- 1. Session watcher: fswatch subprocess alive ---
if pgrep -f "fswatch.*\.jsonl" > /dev/null 2>&1; then
  : # fswatch alive
else
  FAILURES+=("session-watcher: fswatch subprocess dead — sessions not indexing")
fi

# --- 2. Hooks binary matches disk ---
HOOKS_PID=$(cat /tmp/chorus-hooks.pid 2>/dev/null || echo "")
if [ -n "$HOOKS_PID" ] && ps -p "$HOOKS_PID" > /dev/null 2>&1; then
  DISK_MTIME=$(stat -f %m /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hooks 2>/dev/null || echo 0)
  PROC_START=$(ps -p "$HOOKS_PID" -o lstart= 2>/dev/null | xargs -I{} date -j -f "%a %b %d %T %Y" "{}" +%s 2>/dev/null || echo 0)
  if [ "$DISK_MTIME" -gt "$PROC_START" ] 2>/dev/null; then
    FAILURES+=("chorus-hooks: binary on disk newer than running process — rebuild without restart")
  fi
else
  FAILURES+=("chorus-hooks: not running")
fi

# --- 3. Log freshness: alert if 0 bytes or >2h stale ---
LOG_DIR="$HOME/Library/Logs/Chorus"
STALE_2H=$((now - 7200))
STALE_25H=$((now - 90000))

# Skip: stderr-only logs, own log, orphaned logs, agents that don't use stdout
SKIP_LOGS="deep-health.log inject-health.log watchdog.log jeff-input-monitor.log launchagent-metrics.log chorus-bridge.stdout.log"
STDERR_LOGS="clearing-probe-stderr.log chorus-bridge.stderr.log chorus-hooks.stderr.log"
# Daily jobs — 25h threshold instead of 2h
DAILY_LOGS="context-cache-daily.log fuseki-compact.log fuseki-perf.log perf-baseline-nightly.log alert-notifier.log alert-runner.log"

for log in "$LOG_DIR"/*.log; do
  [ -f "$log" ] || continue
  name=$(basename "$log")

  # Skip own log and stderr-only logs
  [[ " $SKIP_LOGS " == *" $name "* ]] && continue
  [[ " $STDERR_LOGS " == *" $name "* ]] && continue

  size=$(stat -f %z "$log" 2>/dev/null || echo 0)
  mtime=$(stat -f %m "$log" 2>/dev/null || echo 0)

  # Pick threshold based on job type
  if [[ " $DAILY_LOGS " == *" $name "* ]]; then
    threshold=$STALE_25H
  else
    threshold=$STALE_2H
  fi

  if [ "$size" -eq 0 ]; then
    FAILURES+=("log-freshness: $name is 0 bytes — agent running but silent")
  elif [ "$mtime" -lt "$threshold" ]; then
    age_h=$(( (now - mtime) / 3600 ))
    FAILURES+=("log-freshness: $name is ${age_h}h stale")
  fi
done

# --- 4. Loki tunnel: SSH exit code ---
if launchctl list 2>/dev/null | grep -q "loki-tunnel-bedroom"; then
  tunnel_exit=$(launchctl list | grep loki-tunnel-bedroom | awk '{print $2}')
  if [ "$tunnel_exit" != "0" ] && [ "$tunnel_exit" != "-" ]; then
    FAILURES+=("loki-tunnel-bedroom: exit code $tunnel_exit — remote log ingestion broken")
  fi
else
  FAILURES+=("loki-tunnel-bedroom: not loaded — remote log ingestion broken")
fi

# --- 5. Chorus index freshness ---
INDEX_DB="$HOME/.chorus/index.db"
if [ -f "$INDEX_DB" ]; then
  idx_mtime=$(stat -f %m "$INDEX_DB" 2>/dev/null || echo 0)
  idx_age=$((now - idx_mtime))
  if [ "$idx_age" -gt 3600 ]; then
    age_h=$((idx_age / 3600))
    FAILURES+=("chorus-index: database ${age_h}h stale — session indexer may be dead")
  fi
fi

# --- 6. Cloudflare tunnel ---
if ! pgrep -f "cloudflared tunnel run" > /dev/null 2>&1; then
  FAILURES+=("cloudflare-tunnel: not running — seeds cannot arrive via SMS")
fi

# --- 7. Chorus API reachable ---
if ! curl -sf --max-time 5 http://localhost:3340/api/chorus/health > /dev/null 2>&1; then
  FAILURES+=("chorus-api: localhost:3340 unreachable — team search, seeds, and context broken")
fi

# --- 8. Gathering app reachable ---
if ! curl -sf --max-time 5 http://localhost:3000/health > /dev/null 2>&1; then
  FAILURES+=("gathering-app: localhost:3000 unreachable — app down")
fi

# --- 9. Nudge delivery ---
if [ ! -x "$NUDGE" ]; then
  FAILURES+=("nudge: binary not found or not executable at $NUDGE")
fi

# --- 10. Critical services: must be loaded and running ---
CRITICAL_SERVICES=(
  # Library — core
  "com.gathering.app:gathering-app:main application"
  "com.gathering.fuseki:fuseki:knowledge graph"
  "com.gathering.vikunja:vikunja:board"
  "com.gathering.css:css:SOLID CSS server"
  "com.gathering.wordpress:wordpress:blog"
  "com.gathering.mysql:mysql:database"
  "com.gathering.messaging:messaging:team messaging"
  # Library — observability
  "com.gathering.loki:loki:log storage"
  "com.gathering.grafana:grafana:dashboards"
  "com.gathering.promtail:promtail:log shipping"
  "com.gathering.prometheus:prometheus:metrics"
  "com.gathering.alertmanager:alertmanager:alert routing"
  "com.gathering.blackbox-exporter:blackbox:endpoint probing"
  "com.gathering.node-exporter:node-exporter:host metrics"
  "com.gathering.mysqld-exporter:mysqld-exporter:mysql metrics"
  # Library — chorus
  "com.chorus.api:chorus-api:team search and context"
  "com.chorus.clearing:clearing:team chat"
  "com.chorus.hooks:chorus-hooks:session hooks"
  "com.chorus.session-watcher:session-watcher:session indexing"
  "com.chorus.alert-notifier:alert-notifier:alert delivery"
  "com.gathering.codebase-graph-watcher:graph-watcher:codebase graph"
)

for entry in "${CRITICAL_SERVICES[@]}"; do
  IFS=: read -r label name desc <<< "$entry"
  line=$(launchctl list 2>/dev/null | grep "$label" || true)
  if [ -z "$line" ]; then
    FAILURES+=("$name: not loaded — $desc down")
  else
    pid=$(echo "$line" | awk '{print $1}')
    exit_code=$(echo "$line" | awk '{print $2}')
    if [ "$pid" = "-" ]; then
      # No PID = not running. Exit 0 means cron that ran fine, skip those.
      if [ "$exit_code" != "0" ]; then
        FAILURES+=("$name: not running (exit $exit_code) — $desc down")
      fi
    fi
  fi
done

# --- 11. HTTP health endpoints ---
# Format: url|name|desc (pipe-delimited to avoid colon collision with URLs)
HEALTH_ENDPOINTS=(
  # Library — core
  "http://localhost:3000/health|gathering-app-http|main app"
  "http://localhost:3340/api/chorus/health|chorus-api-http|team search"
  "http://localhost:3030/$/ping|fuseki-http|SPARQL"
  "http://localhost:3456/api/v1/info|vikunja-http|board API"
  "http://localhost:3470/health|clearing-http|team chat"
  "http://localhost:3475/health|messaging-http|team messaging"
  # Library — observability
  "http://localhost:3102/ready|loki-http|log queries"
  "http://localhost:3100/api/health|grafana-http|dashboards"
  "http://localhost:9090/-/healthy|prometheus-http|metrics queries"
  "http://localhost:9093/-/healthy|alertmanager-http|alert routing"
  "http://localhost:9115/metrics|blackbox-http|endpoint probing"
  "http://localhost:9101/metrics|node-exporter-http|host metrics"
  # Bedroom
  "http://192.168.86.242:3001/|images-api-http|Bedroom photos"
  "http://192.168.86.242:11434/api/tags|ollama-http|semantic search"
)

for entry in "${HEALTH_ENDPOINTS[@]}"; do
  IFS='|' read -r url name desc <<< "$entry"
  if ! curl -sf --max-time 5 "$url" > /dev/null 2>&1; then
    FAILURES+=("$name: unreachable — $desc broken")
  fi
done

# --- 12. (Bedroom endpoints folded into check 11 above) ---

# --- Report ---
if [ ${#FAILURES[@]} -eq 0 ]; then
  echo "deep-health: all checks passed"
  exit 0
fi

MSG="deep-health: ${#FAILURES[@]} failure(s)"
for f in "${FAILURES[@]}"; do
  MSG="$MSG
  - $f"
done

echo "$MSG"
echo "$(date '+%Y-%m-%d %H:%M') $MSG" >> "$HOME/Library/Logs/Chorus/deep-health.log"
"$NUDGE" "$ALERT_ROLE" "$MSG" --force 2>/dev/null || true
"$CHORUS_LOG" ops.health.deep_check_failed "$ALERT_ROLE" failures="${#FAILURES[@]}" 2>/dev/null || true
exit 1
