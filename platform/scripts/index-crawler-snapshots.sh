#!/usr/bin/env bash
# index-crawler-snapshots.sh — Index domain crawler results into Chorus search (#2019)
#
# Calls the crawler API for each active domain, formats results as messages,
# inserts into Chorus SQLite index. The compound loop (context_inject.rs)
# discovers them via existing hybrid search — zero hook code changes.
#
# Usage: index-crawler-snapshots.sh [domain1 domain2 ...]
#   No args = crawl all known domains

set -euo pipefail

API_URL="http://localhost:3340"
DB_PATH="$HOME/.chorus/index.db"
TIMESTAMP=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
STATUS_FILE="/tmp/crawler-domain-status.json"
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

# Load existing status file or start fresh (#1885)
if [ -f "$STATUS_FILE" ]; then
  DOMAIN_STATUS=$(cat "$STATUS_FILE")
else
  DOMAIN_STATUS="{}"
fi

# All crawlable domains — matches knownCrawlDomains in server.ts (#1883, #2026)
ALL_DOMAINS=(
  photos music people books cooking reading watching property stories notes
  blog gallery social glimmers ideas seeds self chorus clearing pulse spine
  interactions memory infrastructure observability loom search
)

DOMAINS=("${@:-${ALL_DOMAINS[@]}}")

if ! sqlite3 "$DB_PATH" "SELECT 1" &>/dev/null; then
  echo "ERROR: Chorus index not accessible at $DB_PATH" >&2
  exit 1
fi

if ! curl -sf "$API_URL/health" -o /dev/null 2>/dev/null; then
  echo "ERROR: Chorus API not reachable at $API_URL" >&2
  exit 1
fi

indexed=0
errors=0

trace_hop() {
  local corr="$1" hop="$2" src="$3" dst="$4" domain="$5" ms="${6:-}"
  curl -s -X POST "$API_URL/api/chorus/trace" \
    -H 'Content-Type: application/json' \
    -d "{\"correlationId\":\"$corr\",\"hop\":$hop,\"callStack\":\"batch\",\"source\":{\"domain\":\"$domain\",\"service\":\"$src\"},\"destination\":{\"domain\":\"$domain\",\"service\":\"$dst\"}${ms:+,\"latencyMs\":$ms}}" \
    --max-time 3 > /dev/null 2>&1 &
}

for domain in "${DOMAINS[@]}"; do
  # Track per-domain timing (#1885)
  domain_start=$(python3 -c "import time; print(int(time.time()*1000))")
  TRACE_ID="crawl-${domain}-$(date +%s)"
  trace_hop "$TRACE_ID" 1 "crawler-start" "crawl-api" "$domain"

  # Call crawler API
  trace_hop "$TRACE_ID" 2 "crawl-api" "chorus-api" "$domain"
  crawl_json=$(curl -sf "$API_URL/api/chorus/crawl/$domain" 2>/dev/null) || {
    domain_end=$(python3 -c "import time; print(int(time.time()*1000))")
    duration_ms=$((domain_end - domain_start))
    prev_failures=$(echo "$DOMAIN_STATUS" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('$domain',{}).get('consecutive_failures',0))" 2>/dev/null || echo "0")
    new_failures=$((prev_failures + 1))
    DOMAIN_STATUS=$(echo "$DOMAIN_STATUS" | python3 -c "
import json,sys
d=json.load(sys.stdin)
d['$domain']={'status':'error','duration_ms':$duration_ms,'timestamp':'$TIMESTAMP','consecutive_failures':$new_failures,'last_error':'crawl API returned non-200'}
print(json.dumps(d))
" 2>/dev/null)
    echo "WARN: Crawl failed for $domain (${duration_ms}ms, ${new_failures} consecutive)" >&2
    "$CHORUS_LOG" crawler.domain.failed system domain="$domain" duration_ms="$duration_ms" consecutive="$new_failures" 2>/dev/null || true
    ((errors++))
    continue
  }

  # Build snapshot summary from crawler response
  snapshot=$(echo "$crawl_json" | python3 -c "
import json, sys

d = json.load(sys.stdin)
domain = d.get('domain', '')
cards = d.get('cards', [])
code_scan = d.get('codeScan', {})
logs = d.get('logs', [])
alerts = d.get('alerts', [])
infra = d.get('infra', {})
history = d.get('history', {})

wip = [c for c in cards if c.get('status') == 'WIP']
done = [c for c in cards if c.get('status') == 'Done']
unresolved = history.get('unresolved', [])

lines = []
lines.append(f'Domain snapshot: {domain}')
lines.append(f'Cards: {len(cards)} total, {len(wip)} WIP, {len(done)} done, {len(unresolved)} unresolved')

# Code files
scanned = code_scan.get('scanned', [])
discovered = code_scan.get('discovered', [])
if scanned or discovered:
    lines.append(f'Code: {len(scanned)} known files, {len(discovered)} discovered')
    for f in scanned[:5]:
        lines.append(f'  {f}')
    if discovered:
        lines.append(f'  + {len(discovered)} more via grep')

# Logs summary
if logs:
    error_count = sum(1 for l in logs if l.get('level') == 'error')
    warn_count = sum(1 for l in logs if l.get('level') == 'warn')
    lines.append(f'Logs (24h): {len(logs)} entries, {error_count} errors, {warn_count} warnings')
    for l in [x for x in logs if x.get('level') == 'error'][:3]:
        lines.append(f'  [error] {l.get(\"message\",\"\")[:100]}')

# Alerts
if alerts:
    lines.append(f'Alerts: {len(alerts)} rules')
    for a in alerts:
        lines.append(f'  {a.get(\"name\",\"?\")} ({a.get(\"severity\",\"?\")})')

# Infrastructure
agents = infra.get('launchagents', [])
endpoints = infra.get('endpoints', [])
if agents:
    lines.append(f'LaunchAgents: {len(agents)}')
if endpoints:
    lines.append(f'Endpoints: {\", \".join(endpoints[:3])}')

# Health
trust = history.get('trust_score', 0)
health = history.get('health', 'unknown')
lines.append(f'Health: {health} (trust score: {trust})')

print('\n'.join(lines))
" 2>/dev/null) || {
    echo "WARN: Snapshot formatting failed for $domain" >&2
    ((errors++))
    continue
  }

  if [ -z "$snapshot" ]; then
    continue
  fi

  # Index into SQLite
  trace_hop "$TRACE_ID" 3 "chorus-api" "sqlite-index" "$domain"
  # Delete previous snapshot for this domain (keep only latest)
  sqlite3 "$DB_PATH" "DELETE FROM messages WHERE source = 'crawler' AND channel = 'crawl:$domain';" 2>/dev/null

  # Escape single quotes for SQL
  escaped_snapshot=$(echo "$snapshot" | sed "s/'/''/g")
  source_id="crawler-${domain}-$(date +%s)"

  # Insert new snapshot
  sqlite3 "$DB_PATH" "INSERT INTO messages (source, source_id, channel, role, author, content, timestamp, metadata)
    VALUES ('crawler', '$source_id', 'crawl:$domain', 'system', 'crawler', '$escaped_snapshot', '$TIMESTAMP', '{\"domain\":\"$domain\"}');" 2>/dev/null

  # Track success (#1885)
  domain_end=$(python3 -c "import time; print(int(time.time()*1000))")
  duration_ms=$((domain_end - domain_start))
  DOMAIN_STATUS=$(echo "$DOMAIN_STATUS" | python3 -c "
import json,sys
d=json.load(sys.stdin)
d['$domain']={'status':'ok','duration_ms':$duration_ms,'timestamp':'$TIMESTAMP','consecutive_failures':0}
print(json.dumps(d))
" 2>/dev/null)
  "$CHORUS_LOG" crawler.domain.indexed system domain="$domain" duration_ms="$duration_ms" 2>/dev/null || true

  trace_hop "$TRACE_ID" 4 "sqlite-index" "crawl-complete" "$domain" "$duration_ms"
  ((indexed++))
  echo "Indexed: $domain ($(echo "$snapshot" | wc -l | tr -d ' ') lines, ${duration_ms}ms)"
done

# Write status file (#1885)
echo "$DOMAIN_STATUS" | python3 -c "import json,sys; print(json.dumps(json.load(sys.stdin), indent=2))" > "$STATUS_FILE" 2>/dev/null

echo ""
echo "Done: $indexed domains indexed, $errors errors"
