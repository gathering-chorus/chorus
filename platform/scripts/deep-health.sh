#!/usr/bin/env bash
# deep-health.sh — subprocess liveness, not wrapper alive (#2228)
# Runs on 5-min cron. Alerts via nudge --force on failure.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

# #2808: bash `nudge` retired in #2804/#2809. Use ops-nudge (pulse-direct).
OPS_NUDGE="${CHORUS_ROOT}/platform/scripts/ops-nudge"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
ALERT_ROLE="silas"
FAILURES=()
WARNINGS=()

now=$(date +%s)

# --- 0. Alert suppression check (#2305) ---
SUPPRESS_FILE="/tmp/chorus-alert-suppress"
if [ -f "$SUPPRESS_FILE" ]; then
  expiry=$(cat "$SUPPRESS_FILE" 2>/dev/null || echo 0)
  if [ "$expiry" -gt "$now" ] 2>/dev/null; then
    remaining=$(( expiry - now ))
    echo "deep-health: suppressed (${remaining}s remaining — planned restart)"
    exit 0
  fi
  # Expired — remove and continue normally
  rm -f "$SUPPRESS_FILE"
fi

# --- 0.5. Fuseki liveness (#2033) ---
FUSEKI_CODE=$(curl -sf --max-time 3 -o /dev/null -w '%{http_code}' "http://localhost:3030/$/ping" 2>/dev/null || echo "000")
if [ "$FUSEKI_CODE" != "200" ]; then
  FAILURES+=("fuseki: localhost:3030 unreachable — ontology, SHACL, domain context broken")
fi

# --- 1. Session watcher: fswatch subprocess alive ---
if pgrep -f "fswatch.*\.jsonl" > /dev/null 2>&1; then
  : # fswatch alive
else
  FAILURES+=("session-watcher: fswatch subprocess dead — sessions not indexing")
fi

# --- 2. Hooks shim binary exists (#2020, #2032) ---
# The shim is invoked per-call by Claude Code, not a persistent daemon.
# No PID check needed — just verify the binary exists and is executable.
# #2478 — resolution via the shared lib (parity-pinned with the TS resolver)
source "$(dirname "$0")/lib/resolve-shim.sh"
SHIM_BIN="$(resolve_shim_path)"
if [ ! -x "$SHIM_BIN" ]; then
  FAILURES+=("chorus-hooks: shim binary missing or not executable")
fi

# --- 3. Log freshness: alert if 0 bytes or >2h stale ---
LOG_DIR="$HOME/Library/Logs/Chorus"
STALE_2H=$((now - 7200))
STALE_25H=$((now - 90000))
STALE_8D=$((now - 691200))

# Skip: stderr-only logs, own log, orphaned logs, agents that don't use stdout
SKIP_LOGS="deep-health.log inject-health.log watchdog.log jeff-input-monitor.log launchagent-metrics.log chorus-bridge.stdout.log chorus-api.log chorus-hooks.stdout.log clearing-probe-stdout.log inject-watcher.log harvest-exporter.log"
STDERR_LOGS="clearing-probe-stderr.log chorus-bridge.stderr.log chorus-hooks.stderr.log"
# Persistent daemons — silence is healthy when the process is alive.
# Format: "logname:launchctl_label" — check PID via launchctl, skip mtime.
# bridge-subscriber-{silas,wren,kade} retired by #3352 (always-inject); their
# leftover logs must not resurrect a liveness expectation (#3369).
LIVENESS_LOGS="chorus-bridge.log:com.chorus.clearing shim-wrapper.log:com.chorus.hooks"
# Daily jobs — 25h threshold instead of 2h
DAILY_LOGS="context-cache-daily.log fuseki-perf.log perf-baseline-nightly.log alert-notifier.log alert-runner.log rsync-backup.log lance-maintain.log"
# Weekly / multi-day jobs — 8 day threshold
WEEKLY_LOGS="context-cache-weekly.log disk-trend.log fuseki-compact.log alert-delivery-test.log cruft-scan.log"

for log in "$LOG_DIR"/*.log; do
  [ -f "$log" ] || continue
  name=$(basename "$log")

  # Skip own log and stderr-only logs
  [[ " $SKIP_LOGS " == *" $name "* ]] && continue
  [[ " $STDERR_LOGS " == *" $name "* ]] && continue

  # Event-driven: check process liveness, not log freshness
  liveness_match=""
  for entry in $LIVENESS_LOGS; do
    lname="${entry%%:*}"
    llabel="${entry##*:}"
    if [ "$name" = "$lname" ]; then
      liveness_match="$llabel"
      break
    fi
  done
  if [ -n "$liveness_match" ]; then
    # grep no-match exits 1; under set -e that killed the whole script with
    # zero output (#3369 — the 6am daily-signal-scan crash). Never let a
    # missing label be fatal; it's a WARNING, not a crash.
    pid=$(launchctl list 2>/dev/null | grep "$liveness_match" | awk '{print $1}' || true)
    if [ "$pid" = "-" ] || [ -z "$pid" ]; then
      WARNINGS+=("liveness: $name — process $liveness_match not running")
    fi
    continue
  fi

  size=$(stat -f %z "$log" 2>/dev/null || echo 0)
  mtime=$(stat -f %m "$log" 2>/dev/null || echo 0)

  # Pick threshold based on job type
  if [[ " $WEEKLY_LOGS " == *" $name "* ]]; then
    threshold=$STALE_8D
  elif [[ " $DAILY_LOGS " == *" $name "* ]]; then
    threshold=$STALE_25H
  else
    threshold=$STALE_2H
  fi

  if [ "$size" -eq 0 ]; then
    WARNINGS+=("log-freshness: $name is 0 bytes — agent running but silent")
  elif [ "$mtime" -lt "$threshold" ]; then
    age_h=$(( (now - mtime) / 3600 ))
    WARNINGS+=("log-freshness: $name is ${age_h}h stale")
  fi
done

# --- 4. Loki reachability from Bedroom (direct LAN, no tunnel — #1988) ---
bedroom_loki=$(ssh -o ConnectTimeout=5 bedroom "curl -sf --max-time 3 http://Jeffs-Mac-Mini-M1-3.local:3102/ready 2>/dev/null" || true)
if [ "$bedroom_loki" != "ready" ]; then
  FAILURES+=("loki-bedroom: Bedroom cannot reach Loki at Jeffs-Mac-Mini-M1-3.local:3102")
fi

# --- 5. Session index freshness (#2270) ---
# Query actual message timestamps, not file mtime. Only alert during working hours.
INDEX_DB="$HOME/.chorus/index.db"
current_hour=$(date +%H)
if [ -f "$INDEX_DB" ] && [ "$current_hour" -ge 8 ] && [ "$current_hour" -lt 22 ]; then
  # Get newest message timestamp from the database (retry once if locked)
  last_ts=$(sqlite3 "$INDEX_DB" "SELECT timestamp FROM messages ORDER BY timestamp DESC LIMIT 1;" 2>/dev/null || echo "")
  if [ -z "$last_ts" ]; then
    sleep 2
    last_ts=$(sqlite3 "$INDEX_DB" "SELECT timestamp FROM messages ORDER BY timestamp DESC LIMIT 1;" 2>/dev/null || echo "")
  fi
  if [ -z "$last_ts" ]; then
    # DB still locked or empty — fall back to file mtime
    idx_mtime=$(stat -f %m "$INDEX_DB" 2>/dev/null || echo 0)
    idx_age=$((now - idx_mtime))
    if [ "$idx_age" -gt 7200 ]; then
      age_h=$((idx_age / 3600))
      FAILURES+=("session-index: DB locked/empty, file ${age_h}h stale. Fix: check fswatch, check lock file")
    fi
  elif [ -n "$last_ts" ]; then
    # Convert ISO timestamp to epoch
    last_epoch=$(date -j -f "%Y-%m-%dT%H:%M:%S" "${last_ts%%.*}" +%s 2>/dev/null || echo 0)
    idx_age=$((now - last_epoch))
    if [ "$idx_age" -gt 7200 ]; then
      age_h=$((idx_age / 3600))
      lockfile_status="no lock"
      [ -f "$HOME/.chorus/watcher.lock" ] && lockfile_status="LOCKED ($(( (now - $(stat -f %m "$HOME/.chorus/watcher.lock")) ))s)"
      fswatch_status="dead"
      pgrep -f "fswatch.*\.jsonl" > /dev/null 2>&1 && fswatch_status="alive"
      FAILURES+=("session-index: last indexed ${age_h}h ago (${last_ts}). fswatch: ${fswatch_status}, lockfile: ${lockfile_status}. Fix: check fswatch, check lock file, run chorus-index.sh sessions")
    fi
  else
    FAILURES+=("session-index: no messages in index.db — indexer never ran or DB corrupt")
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
# Post #2122: Gathering lives on :3001; Caddy edge-proxy owns :3000. /health
# returns 401 unauthorized by design (auth-gated), so check / for 200/301/302.
GATHERING_CODE=$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' http://localhost:3002/ 2>/dev/null)
if ! [[ "$GATHERING_CODE" =~ ^(200|204|301|302)$ ]]; then
  FAILURES+=("gathering-app: localhost:3002 returned ${GATHERING_CODE:-000} — app down (post-#2122 moved from :3000 to :3002 when Caddy took :3000)")
fi
# Caddy edge-proxy (#2122) fronts :3000, routes /borg/* to chorus-api.
CADDY_CODE=$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' http://localhost:3000/ 2>/dev/null)
if ! [[ "$CADDY_CODE" =~ ^(200|204|301|302)$ ]]; then
  FAILURES+=("caddy-edge: localhost:3000 returned ${CADDY_CODE:-000} — edge proxy down, inbound bookmarks broken")
fi

# --- 9. Nudge delivery (post-#2808: ops-nudge wrapper, not bash nudge) ---
if [ ! -x "$OPS_NUDGE" ]; then
  FAILURES+=("ops-nudge: helper not found or not executable at $OPS_NUDGE")
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

# --- 10b. Completeness: every chorus agent on disk MUST be loaded (#3444) ---
# No hardcoded allowlist to drift: the plist's EXISTENCE is the declaration
# "should run". Retiring an agent = DELETE its plist, so on-disk-but-not-loaded
# is always an anomaly. Catches the silent deaths (embed-worker, watchdog, ...)
# that the curated CRITICAL_SERVICES list (#10) structurally cannot see.
for plist in "$HOME"/Library/LaunchAgents/com.chorus.*.plist; do
  [ -f "$plist" ] || continue
  label=$(basename "$plist" .plist)
  if ! launchctl list "$label" >/dev/null 2>&1; then
    FAILURES+=("$label: plist on disk but NOT loaded — silent death (load it, or delete the plist if retired)")
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
  "http://Jeffs-Mac-mini.local:3001/|images-api-http|Bedroom photos"
  "http://Jeffs-Mac-mini.local:11434/api/tags|ollama-http|semantic search"
)

# --- 11b. LanceDB sync drift (#1920 — replaces mtime check) ---
# Check producer-consumer gap: SQLite rows vs LanceDB vectors.
# Drift > 0 with no activity = fine. Drift > threshold = degraded search.
LANCE_DRIFT_THRESHOLD=5000
lance_health=$(curl -sf --max-time 5 http://localhost:3340/api/chorus/health/detail 2>/dev/null || echo "")
if [ -n "$lance_health" ]; then
  lance_unembedded=$(echo "$lance_health" | python3 -c "import sys,json; print(json.load(sys.stdin).get('unembedded',0))" 2>/dev/null || echo 0)
  if [ "$lance_unembedded" -gt "$LANCE_DRIFT_THRESHOLD" ]; then
    FAILURES+=("lancedb: ${lance_unembedded} unembedded messages — semantic search degraded")
  fi
fi

# --- 11c. Vikunja log freshness (#1856) ---
VIKUNJA_LOG="/Users/jeffbridwell/Library/Logs/Gathering/vikunja.log"
if [ ! -f "$VIKUNJA_LOG" ]; then
  WARNINGS+=("vikunja: log file missing")
elif [ -z "$(find "$VIKUNJA_LOG" -mmin -60 2>/dev/null)" ]; then
  WARNINGS+=("vikunja: log not updated in 1h — service may be stalled")
fi

# --- 11d. Vikunja auth probe — cards CLI 401 detection (#2147) ---
# `timeout` does not exist on this machine (no coreutils) — the old
# `timeout 8 cards ...` exit-127'd, and under set -e a failing $() assignment
# is fatal, so every run died HERE with no output and the auth check below
# was unreachable (#3369). Guard the substitution; capture the real exit.
CARDS_EXIT=0
CARDS_BIN="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/cards"
[ -x "$CARDS_BIN" ] || CARDS_BIN="cards"   # PATH fallback for symlinked deploys
CARDS_OUT=$("$CARDS_BIN" list --limit 1 2>&1) || CARDS_EXIT=$?
# Anchor on the API's auth-error shape, not bare "401|403" — card titles,
# latencies (6.401ms), and ids substring-matched and fired false alarms (#3369).
if echo "$CARDS_OUT" | grep -qE "status[= ]40[13]|code=40[13]|HTTP[/ ]?[0-9.]* ?40[13]|Unauthorized|Forbidden|invalid token"; then
  # REAL auth failure — the API answered with 401/403. This is the token alarm.
  FAILURES+=("vikunja-auth: cards CLI auth failure — check VIKUNJA_API_TOKEN (env unset, token expired, or wrapper edited)")
elif [ "$CARDS_EXIT" -eq 127 ]; then
  # 127 = command-not-found: the cards CLI (or one of its deps) isn't on THIS
  # run's PATH — the cron-context probe-misconfig (#3405). That is a PROBE
  # problem, not a Vikunja outage; vikunja-http (check 11) owns reachability.
  # Distinguishing probe-misconfig from service-down is ADR-043's contract.
  WARNINGS+=("vikunja-auth-probe: cards CLI not runnable here (exit 127 — PATH/dep) — probe-misconfig, NOT a Vikunja failure")
elif [ "$CARDS_EXIT" -ne 0 ]; then
  # Other nonzero with no auth-error shape: inconclusive. Warn, never blanket-
  # claim 'Vikunja down' (vikunja-http already covers a real outage).
  WARNINGS+=("vikunja-auth-probe: cards CLI exit ${CARDS_EXIT} with no auth-error in output — probe inconclusive")
fi

for entry in "${HEALTH_ENDPOINTS[@]}"; do
  IFS='|' read -r url name desc <<< "$entry"
  if ! curl -sf --max-time 5 "$url" > /dev/null 2>&1; then
    FAILURES+=("$name: unreachable — $desc broken")
  fi
done

# --- 12. (Bedroom endpoints folded into check 11 above) ---

# --- 13. Alert delivery test freshness (#2274) ---
DELIVERY_LOG="$HOME/Library/Logs/Chorus/alert-delivery-test.log"
STALE_8D=$((now - 691200))
if [ -f "$DELIVERY_LOG" ]; then
  delivery_mtime=$(stat -f %m "$DELIVERY_LOG" 2>/dev/null || echo 0)
  if [ "$delivery_mtime" -lt "$STALE_8D" ]; then
    delivery_age_d=$(( (now - delivery_mtime) / 86400 ))
    FAILURES+=("alert-delivery: last run ${delivery_age_d}d ago — weekly cron may be dead. Fix: bash platform/scripts/alert-delivery-test.sh")
  else
    # Read the LATEST run's verdict, not a whole-file FAIL scan. The old check
    # grepped the entire log for FAIL and took the last match — which surfaced
    # the OLDEST-surviving FAIL (a 2026-05-12 entry) while today's run passed
    # (#3405). The test writes one `RESULT:` line per run; the last is current.
    last_result=$(grep "RESULT:" "$DELIVERY_LOG" | tail -1)
    if [ -n "$last_result" ] && ! echo "$last_result" | grep -q "all passed"; then
      FAILURES+=("alert-delivery: latest run did not pass — ${last_result}")
    fi
  fi
else
  FAILURES+=("alert-delivery: test never run — run platform/scripts/alert-delivery-test.sh to baseline")
fi

# --- 14. Dashboard content validation (#2278) — weekly ---
DASH_HEALTH_LOG="$HOME/Library/Logs/Chorus/dashboard-health.log"
if [ -f "$DASH_HEALTH_LOG" ]; then
  dash_mtime=$(stat -f %m "$DASH_HEALTH_LOG" 2>/dev/null || echo 0)
  if [ "$dash_mtime" -lt "$STALE_8D" ]; then
    dash_age_d=$(( (now - dash_mtime) / 86400 ))
    FAILURES+=("dashboard-health: last run ${dash_age_d}d ago — weekly check may be dead")
  elif grep -q "FAIL" "$DASH_HEALTH_LOG" 2>/dev/null; then
    last_fail=$(grep "FAIL" "$DASH_HEALTH_LOG" | tail -1)
    FAILURES+=("dashboard-health: $last_fail")
  fi
fi

# --- 15. Standards surface freshness (#2268; #3405 re-home) ---
# The chorus-standards.html surface was retired off gathering-docs by the
# chorus-out-of-gathering migration (#2969/#3361). The generator is a transform
# whose seed moved; the artifact has no current home until it re-lands in the
# chorus-api tree with #3361. Until then a missing/stale surface is a tracked
# WARNING (a known migration gap), NOT a service-down FAILURE that nudges
# nightly. The probe target follows the artifact once #3361 re-homes it.
STANDARDS_HTML="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/chorus-standards.html"
STALE_48H=$((now - 172800))
if [ -f "$STANDARDS_HTML" ]; then
  standards_mtime=$(stat -f %m "$STANDARDS_HTML" 2>/dev/null || echo 0)
  if [ "$standards_mtime" -lt "$STALE_48H" ]; then
    standards_age_h=$(( (now - standards_mtime) / 3600 ))
    WARNINGS+=("standards-surface: chorus-standards.html is ${standards_age_h}h stale (#3361 re-home pending) — regenerate, or migrate the probe target to the chorus-api home.")
  fi
else
  WARNINGS+=("standards-surface: chorus-standards.html absent at the gathering path — retired by the chorus-out-of-gathering migration (#3361), not yet re-homed. Probe target follows the artifact once it lands in chorus-api.")
fi

# --- 15b. Borg page data-present probes (#2124) ---
# /borg/* pages return 200 even when aggregators error. borg-health-check.sh
# probes each backing API against a contract and alerts when data assertions
# fail, not just when HTTP fails.
BORG_HEALTH="${CHORUS_ROOT}/platform/scripts/borg-health-check.sh"
if [ -x "$BORG_HEALTH" ]; then
  while IFS= read -r line; do
    case "$line" in
      FAIL*)
        # New, untracked failure — real alert
        FAILURES+=("borg-health: ${line#FAIL }")
        ;;
      WARN*)
        # Known failure, tracked by a card — status visible, no nudge
        WARNINGS+=("borg-health: ${line#WARN }")
        ;;
    esac
  done < <("$BORG_HEALTH" 2>/dev/null || true)
fi

# --- 16. Shadow logs in /tmp/ (DEC-114) ---
TMP_LOGS=$(find /tmp -maxdepth 1 -name "*.log" -size +0c 2>/dev/null | wc -l | tr -d ' ')
if [ "$TMP_LOGS" -gt 0 ]; then
  TMP_LIST=$(find /tmp -maxdepth 1 -name "*.log" -size +0c -exec basename {} \; 2>/dev/null | head -5 | tr '\n' ', ' | sed 's/,$//')
  WARNINGS+=("shadow-logs: ${TMP_LOGS} log file(s) in /tmp/ — DEC-114 violation: ${TMP_LIST}")
fi

# --- 17. Bind-posture: internal services must not listen on 0.0.0.0 (#3390, ADR-012 intent) ---
# A decision made in ADR-012 (bind localhost) was lost in the ADR-019 native
# migration. This guard makes the regression visible + un-losable: internal
# services that have NO cross-machine consumer must bind 127.0.0.1, never *.
# LAN-EXCEPTION ports (documented, allowed on 0.0.0.0): 3340 chorus-api
# (Bedroom→Library ops), 3102 loki (Bedroom promtail), 3471 clearing-HTTPS
# (LAN mic #1782), 3000 caddy (tunnel front). Everything else internal = localhost.
LOCALHOST_ONLY_PORTS="3344 3352 3475 3030 3306"  # mcp, messaging, fuseki, mysqld (NOT 3470: clearing serves LAN intentionally for the phone URL #3366 — its hole is unauth-LAN, an auth-model question, not bind)
for port in $LOCALHOST_ONLY_PORTS; do
  # match a listener bound to all-interfaces (*:port or 0.0.0.0:port), not 127.0.0.1
  if lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | grep -qE "(\*|0\.0\.0\.0):${port}\b"; then
    svc=$(lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | awk 'NR==2{print $1}')
    WARNINGS+=("bind-posture: ${svc:-service} on :${port} listens on 0.0.0.0 — internal service should bind 127.0.0.1 (#3390/ADR-012). Set CHORUS_BIND=127.0.0.1.")
  fi
done

# --- 18. Fuseki anon-write LOCK (#3630) — the store must refuse unauthenticated writes ---
# The 62GB source-of-truth store accepts anonymous localhost writes today (204,
# proven live 2026-07-09). This probe is the acceptance oracle for #3630: it
# attempts an UNAUTHENTICATED SPARQL UPDATE into a throwaway graph. If the write
# SUCCEEDS (2xx) the anon hole is OPEN — a FAILURE that stays red until the LOCK
# lands and cannot silently reopen. If it's REFUSED (401/403) the LOCK holds.
# Post-flip the INSERT is refused, so this probe writes NOTHING to the store; it
# only self-cleans the throwaway graph on the pre-lock path where the write lands.
ANON_PROBE_GRAPH="urn:chorus:health/anon-write-probe"
ANON_CODE=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
  -X POST "http://localhost:3030/pods/update" \
  -H "Content-Type: application/sparql-update" \
  --data-binary "INSERT DATA { GRAPH <${ANON_PROBE_GRAPH}> { <urn:chorus:health/probe> <urn:chorus:health/at> \"deep-health\" } }" 2>/dev/null || echo "000")
case "$ANON_CODE" in
  2*)
    # The anon write landed — the hole is open. Clean up the probe triple (also
    # anon; it works precisely because the hole is still open).
    curl -s -o /dev/null --max-time 5 -X POST "http://localhost:3030/pods/update" \
      -H "Content-Type: application/sparql-update" \
      --data-binary "DROP GRAPH <${ANON_PROBE_GRAPH}>" 2>/dev/null || true
    FAILURES+=("fuseki-anon-write: :3030/pods/update accepted an UNAUTHENTICATED write (HTTP ${ANON_CODE}) — the 62GB source-of-truth store's anon-write LOCK is OPEN (#3630). Any local process can mutate the graph with no credential.")
    ;;
  401|403)
    : # refused — the LOCK holds. Healthy.
    ;;
  *)
    WARNINGS+=("fuseki-anon-write-probe: inconclusive (HTTP ${ANON_CODE:-000}) — could not confirm the store rejects anon writes (#3630 oracle). Fuseki reachability is owned by check 0.5/11.")
    ;;
esac

# --- Report ---
# Determine status: degraded (real failures), warning (only log-freshness), healthy (nothing)
if [ ${#FAILURES[@]} -gt 0 ]; then
  STATUS="degraded"
elif [ ${#WARNINGS[@]} -gt 0 ]; then
  STATUS="warning"
else
  STATUS="healthy"
fi

# Write JSON for pulse.rs consumption
{
  echo -n '{"status":"'"$STATUS"'","failures":'"${#FAILURES[@]}"',"warning_count":'"${#WARNINGS[@]}"',"details":['
  sep=""
  if [ ${#FAILURES[@]} -gt 0 ]; then
    for f in "${FAILURES[@]}"; do
      echo -n "${sep}\"$(echo "$f" | sed 's/"/\\"/g')\""
      sep=","
    done
  fi
  echo -n '],"warnings":['
  sep=""
  if [ ${#WARNINGS[@]} -gt 0 ]; then
    for w in "${WARNINGS[@]}"; do
      echo -n "${sep}\"$(echo "$w" | sed 's/"/\\"/g')\""
      sep=","
    done
  fi
  echo -n '],"timestamp":"'"$(date -u '+%Y-%m-%dT%H:%M:%SZ')"'"}'
} > /tmp/deep-health-latest.json

if [ "$STATUS" = "healthy" ]; then
  echo "deep-health: all checks passed"
  exit 0
fi

if [ "$STATUS" = "warning" ]; then
  echo "deep-health: ${#WARNINGS[@]} warning(s) (no failures)"
  echo "$(date '+%Y-%m-%d %H:%M') deep-health: ${#WARNINGS[@]} warning(s)" >> "$HOME/Library/Logs/Chorus/deep-health.log"
  exit 0
fi

# degraded — real failures
MSG="deep-health: ${#FAILURES[@]} failure(s)"
for f in "${FAILURES[@]}"; do
  MSG="$MSG
  - $f"
done

echo "$MSG"
echo "$(date '+%Y-%m-%d %H:%M') $MSG" >> "$HOME/Library/Logs/Chorus/deep-health.log"

# Edge-triggered alerting (#2124 follow-up):
# Only nudge when the failure set changes (new failure added or existing one
# resolved). Steady-state repeated failures are logged but not re-injected —
# they're already tracked by swat cards and the status is visible in pulse.
STATE_FILE="/tmp/deep-health-last-failures.txt"
CURRENT=$(printf '%s\n' "${FAILURES[@]}" | sort)
LAST=""
[ -f "$STATE_FILE" ] && LAST=$(cat "$STATE_FILE")
printf '%s' "$CURRENT" > "$STATE_FILE"

if [ "$CURRENT" = "$LAST" ]; then
  echo "deep-health: failure set unchanged — alert suppressed (see $STATE_FILE)"
else
  "$OPS_NUDGE" "$ALERT_ROLE" "$MSG" 2>/dev/null || true
fi
"$CHORUS_LOG" ops.health.deep_check_failed "$ALERT_ROLE" failures="${#FAILURES[@]}" 2>/dev/null || true
exit 1
