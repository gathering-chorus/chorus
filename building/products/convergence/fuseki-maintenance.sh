#!/usr/bin/env bash
# fuseki-maintenance.sh — Fuseki TDB2 DBA maintenance
#
# Usage:
#   fuseki-maintenance.sh check           — Health check (write test per domain + store stats)
#   fuseki-maintenance.sh compact         — Trigger TDB2 compact via admin API
#   fuseki-maintenance.sh compact-status  — Check if compact is needed / last run
#   fuseki-maintenance.sh backup          — Backup TDB2 volume to tarball
#   fuseki-maintenance.sh rebuild         — Nuclear: stop, delete volume, restart (re-sync from TTL)
#   fuseki-maintenance.sh text-index      — Rebuild Lucene text index
#
# Card: #521

set -u

FUSEKI_QUERY="http://localhost:3030/pods/query"
FUSEKI_UPDATE="http://localhost:3030/pods/update"
FUSEKI_ADMIN="http://localhost:3030/\$"
ENV_FILE="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env"
APP_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
BACKUP_DIR="$CHORUS_ROOT/roles/silas/backups"
# #2119 — Fuseki now runs as a native LaunchAgent (com.gathering.fuseki).
# TDB2 data lives on the host filesystem, managed via launchctl, not docker.
FUSEKI_DATA_DIR="${HOME}/.gathering/data/fuseki-pods"
FUSEKI_LABEL="com.gathering.fuseki"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

FUSEKI_PW=""
if [ -f "$ENV_FILE" ]; then
  FUSEKI_PW=$(grep -m1 '^FUSEKI_ADMIN_PASSWORD=' "$ENV_FILE" | cut -d= -f2 | tr -d '"' 2>/dev/null || true)
fi

auth_flag=""
[ -n "$FUSEKI_PW" ] && auth_flag="-u admin:$FUSEKI_PW"

log_event() {
  [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" "$1" silas "$2" 2>/dev/null || true
}

sparql() {
  curl -s --max-time 15 "$FUSEKI_QUERY" \
    -H 'Content-Type: application/sparql-query' \
    -H 'Accept: application/sparql-results+json' \
    -d "$1"
}

# ── check ──────────────────────────────────────────────────────
cmd_check() {
  echo "Fuseki Health Check"
  echo "==================="
  echo ""

  # 1. Ping
  local ping_code
  ping_code=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "$FUSEKI_ADMIN/ping")
  if [ "$ping_code" = "200" ]; then
    echo -e "  ${GREEN}PASS${NC}  Fuseki responding (ping $ping_code)"
  else
    echo -e "  ${RED}FAIL${NC}  Fuseki not responding (ping $ping_code)"
    echo "  Cannot proceed with health checks."
    exit 2
  fi

  # 2. LaunchAgent status (#2119 — migrated from docker ps)
  local la_status
  la_status=$(launchctl list 2>/dev/null | awk -v label="$FUSEKI_LABEL" '$3 == label {print $1 "|" $2}')
  local la_pid="${la_status%%|*}"
  local la_exit="${la_status##*|}"
  if [ "$la_pid" != "-" ] && [ -n "$la_pid" ]; then
    echo -e "  ${GREEN}PASS${NC}  LaunchAgent running — pid=$la_pid"
  else
    echo -e "  ${YELLOW}WARN${NC}  LaunchAgent not running (last exit=$la_exit)"
  fi

  # 3. Store stats
  echo ""
  echo "Store Statistics:"
  local graph_count
  graph_count=$(sparql 'SELECT (COUNT(DISTINCT ?g) AS ?c) WHERE { GRAPH ?g { ?s ?p ?o } }' \
    | jq -r '.results.bindings[0].c.value' 2>/dev/null)
  echo "  Graphs: ${graph_count:-unknown}"

  # Count per domain prefix
  local PREFIXES=("music" "photos" "media" "stories" "notes" "blog" "ontology" "books")
  for domain in "${PREFIXES[@]}"; do
    local count
    count=$(sparql "SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH ?g { ?s a ?t } FILTER(STRSTARTS(STR(?g), \"http://localhost:3000/pods/jeff/$domain\")) }" \
      | jq -r '.results.bindings[0].c.value' 2>/dev/null)
    [ "${count:-0}" != "0" ] && echo "  $domain: $count resources"
  done

  # 4. Write test per domain
  echo ""
  echo "Write Tests:"
  local test_pass=0
  local test_fail=0
  local test_domains=("music" "photos" "media" "stories" "notes")

  for domain in "${test_domains[@]}"; do
    local test_graph="http://localhost:3000/pods/jeff/$domain/__health_check__.ttl"
    local ttl="@prefix jb: <https://jeffbridwell.com/ontology#> . <urn:health:$domain> a jb:HarvestRun ."

    local write_code
    write_code=$(echo "$ttl" | curl -s --max-time 10 $auth_flag \
      -X PUT \
      -H 'Content-Type: text/turtle' \
      --data-binary @- \
      -o /dev/null -w "%{http_code}" \
      "$FUSEKI_QUERY/../data?graph=$test_graph" 2>/dev/null)

    if [ "$write_code" = "200" ] || [ "$write_code" = "201" ]; then
      # Clean up
      curl -s --max-time 5 $auth_flag -X DELETE \
        "$FUSEKI_QUERY/../data?graph=$test_graph" -o /dev/null 2>/dev/null
      echo -e "  ${GREEN}PASS${NC}  $domain: write OK"
      test_pass=$((test_pass + 1))
    else
      echo -e "  ${RED}FAIL${NC}  $domain: write failed ($write_code) — possible NodeTable corruption"
      test_fail=$((test_fail + 1))
    fi
  done

  echo ""
  if [ "$test_fail" -gt 0 ]; then
    echo -e "  ${RED}$test_fail write failures — run 'fuseki-maintenance.sh compact' or 'rebuild'${NC}"
    log_event "ops.fuseki.health" "status=fail,write_fail=$test_fail,write_pass=$test_pass"
    exit 2
  else
    echo -e "  ${GREEN}All write tests passed${NC}"
    log_event "ops.fuseki.health" "status=ok,graphs=$graph_count,write_pass=$test_pass"
  fi
}

# ── compact ────────────────────────────────────────────────────
cmd_compact() {
  echo "TDB2 Compact"
  echo "============="
  echo ""

  # Check Fuseki is up
  local ping_code
  ping_code=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "$FUSEKI_ADMIN/ping")
  if [ "$ping_code" != "200" ]; then
    echo -e "  ${RED}FAIL${NC}  Fuseki not responding"
    exit 2
  fi

  echo "Starting compact (this may take several minutes for large stores)..."
  local result
  result=$(curl -s --max-time 30 $auth_flag -X POST \
    "$FUSEKI_ADMIN/compact/pods?deleteOld=true" 2>/dev/null)

  local task_id
  task_id=$(echo "$result" | jq -r '.taskId // empty' 2>/dev/null)

  if [ -n "$task_id" ]; then
    echo -e "  ${GREEN}OK${NC}  Compact started (taskId: $task_id)"
    echo "  Monitor: curl $auth_flag '$FUSEKI_ADMIN/tasks/$task_id'"
    log_event "ops.fuseki.compact.started" "task_id=$task_id"

    # Poll for completion (up to 10 minutes)
    echo "  Waiting for completion..."
    local elapsed=0
    while [ $elapsed -lt 600 ]; do
      sleep 15
      elapsed=$((elapsed + 15))
      local task_status
      task_status=$(curl -s --max-time 10 $auth_flag "$FUSEKI_ADMIN/tasks/$task_id" 2>/dev/null)
      if echo "$task_status" | jq -e '.finished' >/dev/null 2>&1; then
        local finished
        finished=$(echo "$task_status" | jq -r '.finished')
        echo -e "  ${GREEN}DONE${NC}  Compact finished at $finished (${elapsed}s)"
        log_event "ops.fuseki.compact.completed" "task_id=$task_id,duration=${elapsed}s"

        # Record last compact time
        echo "$finished" > /Users/jeffbridwell/Library/Logs/Gathering/fuseki-last-compact
        return 0
      fi
      echo "    ...${elapsed}s elapsed"
    done

    echo -e "  ${YELLOW}TIMEOUT${NC}  Compact still running after 10 minutes. Check manually."
    log_event "ops.fuseki.compact.timeout" "task_id=$task_id"
  else
    echo -e "  ${RED}FAIL${NC}  Compact request failed: $result"
    exit 2
  fi
}

# ── compact-status ─────────────────────────────────────────────
cmd_compact_status() {
  echo "Compact Status"
  echo "=============="
  echo ""

  if [ -f /Users/jeffbridwell/Library/Logs/Gathering/fuseki-last-compact ]; then
    local last
    last=$(cat /Users/jeffbridwell/Library/Logs/Gathering/fuseki-last-compact)
    echo "  Last compact: $last"

    # Calculate days since
    local last_epoch
    last_epoch=$(date -j -f '%Y-%m-%dT%H:%M:%S' "${last%%.*}" '+%s' 2>/dev/null || echo "0")
    local now_epoch
    now_epoch=$(date '+%s')
    local days_ago=$(( (now_epoch - last_epoch) / 86400 ))

    if [ "$days_ago" -le 7 ]; then
      echo -e "  ${GREEN}OK${NC}  Compacted ${days_ago}d ago"
    elif [ "$days_ago" -le 30 ]; then
      echo -e "  ${YELLOW}WARN${NC}  Compacted ${days_ago}d ago — consider running compact"
    else
      echo -e "  ${RED}OVERDUE${NC}  Compacted ${days_ago}d ago — run compact"
    fi
  else
    echo -e "  ${YELLOW}UNKNOWN${NC}  No compact history found"
    echo "  Run 'fuseki-maintenance.sh compact' to establish baseline"
  fi

  # Check for running compact tasks
  local tasks
  tasks=$(curl -s --max-time 5 $auth_flag "$FUSEKI_ADMIN/tasks" 2>/dev/null)
  if [ -n "$tasks" ] && echo "$tasks" | jq -e '.[0]' >/dev/null 2>&1; then
    echo ""
    echo "  Active tasks:"
    echo "$tasks" | jq -r '.[] | "    \(.taskId): \(.task) — started \(.started)"' 2>/dev/null
  fi
}

# ── backup ─────────────────────────────────────────────────────
cmd_backup() {
  echo "TDB2 Backup"
  echo "==========="
  echo ""

  mkdir -p "$BACKUP_DIR"
  local timestamp
  timestamp=$(date '+%Y%m%d-%H%M')
  local backup_file="$BACKUP_DIR/fuseki-backup-$timestamp.tar.gz"

  # #2119 — native TDB2 directory backup (no docker volume)
  if [ ! -d "$FUSEKI_DATA_DIR" ]; then
    echo -e "  ${RED}FAIL${NC}  Data directory $FUSEKI_DATA_DIR not found"
    exit 2
  fi

  echo "  Backing up $FUSEKI_DATA_DIR to $backup_file..."
  tar czf "$backup_file" -C "$(dirname "$FUSEKI_DATA_DIR")" "$(basename "$FUSEKI_DATA_DIR")" 2>/dev/null

  local size
  size=$(ls -lh "$backup_file" | awk '{print $5}')
  echo -e "  ${GREEN}OK${NC}  Backup complete: $backup_file ($size)"
  log_event "ops.fuseki.backup.completed" "file=$backup_file,size=$size"

  # Prune old backups (keep last 5)
  local backup_count
  backup_count=$(ls -1 "$BACKUP_DIR"/fuseki-backup-*.tar.gz 2>/dev/null | wc -l | tr -d ' ')
  if [ "$backup_count" -gt 5 ]; then
    local to_prune=$((backup_count - 5))
    ls -1t "$BACKUP_DIR"/fuseki-backup-*.tar.gz | tail -"$to_prune" | while IFS= read -r old; do
      rm -f "$old"
      echo "  Pruned old backup: $(basename "$old")"
    done
  fi
}

# ── rebuild ────────────────────────────────────────────────────
cmd_rebuild() {
  echo "TDB2 Full Rebuild"
  echo "================="
  echo ""
  echo -e "  ${RED}WARNING: This deletes the TDB2 store and rebuilds from TTL files.${NC}"
  echo "  The app's FusekiSyncService will re-populate on startup."
  echo "  Sexuality/media data must be re-loaded via harvest-media.sh."
  echo ""

  # Safety: require --confirm flag
  if [ "${1:-}" != "--confirm" ]; then
    echo "  To proceed: fuseki-maintenance.sh rebuild --confirm"
    exit 1
  fi

  log_event "ops.fuseki.rebuild.started" "reason=manual"

  # #2119 — native rebuild via launchctl, not docker compose
  echo "  1/3  Stopping Fuseki LaunchAgent..."
  bash "$CHORUS_ROOT/platform/scripts/agent-state.sh" stop fuseki 2>&1 | grep -v '^$' || true

  echo "  2/3  Deleting TDB2 data directory..."
  if [ -d "$FUSEKI_DATA_DIR" ]; then
    rm -rf "$FUSEKI_DATA_DIR"
  fi

  echo "  3/3  Starting Fuseki LaunchAgent (fresh store)..."
  bash "$CHORUS_ROOT/platform/scripts/agent-state.sh" start fuseki 2>&1 | grep -v '^$'

  # #2119 — wait for healthy via HTTP ping (no docker healthcheck)
  echo "  Waiting for Fuseki to respond..."
  local elapsed=0
  while [ $elapsed -lt 120 ]; do
    sleep 5
    elapsed=$((elapsed + 5))
    local ping_code
    ping_code=$(curl -sf --max-time 3 -o /dev/null -w '%{http_code}' "http://localhost:3030/\$/ping" 2>/dev/null)
    if [ "$ping_code" = "200" ]; then
      echo -e "  ${GREEN}OK${NC}  Fuseki healthy after ${elapsed}s"
      log_event "ops.fuseki.rebuild.completed" "duration=${elapsed}s"
      echo ""
      echo "  Store is empty. App will auto-sync pod data on next request."
      echo "  For sexuality data: run architect/scripts/harvest-media.sh"
      return 0
    fi
  done

  echo -e "  ${RED}TIMEOUT${NC}  Fuseki not healthy after 120s. Check logs: ~/Library/Logs/Gathering/fuseki.log"
  exit 2
}

# ── text-index ─────────────────────────────────────────────────
cmd_text_index() {
  echo "Lucene Text Index Rebuild"
  echo "========================="
  echo ""

  # #2119 — delete sentinel on native filesystem, restart via launchctl
  echo "  Removing text index sentinel to trigger rebuild on next restart..."
  rm -f "$FUSEKI_DATA_DIR/databases/.text-index-built" 2>/dev/null

  echo "  Restarting Fuseki LaunchAgent to trigger index rebuild..."
  bash "$CHORUS_ROOT/platform/scripts/agent-state.sh" restart fuseki 2>&1 | grep -v '^$'

  echo -e "  ${GREEN}OK${NC}  Fuseki restarting — entrypoint will rebuild Lucene index before serving."
  echo "  This may take 1-5 minutes depending on store size."
  log_event "ops.fuseki.textindex.started" "trigger=manual"
}

# ── main ───────────────────────────────────────────────────────
case "${1:-help}" in
  check)          cmd_check ;;
  compact)        cmd_compact ;;
  compact-status) cmd_compact_status ;;
  backup)         cmd_backup ;;
  rebuild)        shift; cmd_rebuild "${1:-}" ;;
  text-index)     cmd_text_index ;;
  help|*)
    echo "fuseki-maintenance.sh — Fuseki TDB2 DBA maintenance"
    echo ""
    echo "Commands:"
    echo "  check           Health check (write test per domain + store stats)"
    echo "  compact         Trigger TDB2 compact (defrag + index rebuild)"
    echo "  compact-status  Check when compact last ran"
    echo "  backup          Backup TDB2 volume to tarball"
    echo "  rebuild         Nuclear: delete store, rebuild from TTL (requires --confirm)"
    echo "  text-index      Rebuild Lucene text index"
    ;;
esac
