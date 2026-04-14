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

for domain in "${DOMAINS[@]}"; do
  # Call crawler API
  crawl_json=$(curl -sf "$API_URL/api/chorus/crawl/$domain" 2>/dev/null) || {
    echo "WARN: Crawl failed for $domain" >&2
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

  # Delete previous snapshot for this domain (keep only latest)
  sqlite3 "$DB_PATH" "DELETE FROM messages WHERE source = 'crawler' AND channel = 'crawl:$domain';" 2>/dev/null

  # Escape single quotes for SQL
  escaped_snapshot=$(echo "$snapshot" | sed "s/'/''/g")
  source_id="crawler-${domain}-$(date +%s)"

  # Insert new snapshot
  sqlite3 "$DB_PATH" "INSERT INTO messages (source, source_id, channel, role, author, content, timestamp, metadata)
    VALUES ('crawler', '$source_id', 'crawl:$domain', 'system', 'crawler', '$escaped_snapshot', '$TIMESTAMP', '{\"domain\":\"$domain\"}');" 2>/dev/null

  ((indexed++))
  echo "Indexed: $domain ($(echo "$snapshot" | wc -l | tr -d ' ') lines)"
done

echo ""
echo "Done: $indexed domains indexed, $errors errors"
