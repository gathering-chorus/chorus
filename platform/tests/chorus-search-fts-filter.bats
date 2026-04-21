#!/usr/bin/env bats
# chorus-search-fts-filter.bats — #2323
# What Jeff sees: chorus search returns real content, not its own telemetry.
# Queries for "wren last session shipped friction decisions" and
# "sequence:clearing" must return zero self-referential search.query.executed
# rows after this fix lands.

INDEX_DB="$HOME/.chorus/index.db"
INDEXER="$HOME/.chorus/scripts/chorus-index.sh"

# --- AC: no self-referential telemetry in the index after purge ---

@test "index.db has zero rows where the spine event IS search.query.executed (actual event field, not substring)" {
  # Substring match is too broad — it catches legit card.item.commented,
  # observer.digest, card.pulled events that mention the string in content.
  # The real signal is: how many spine rows have event == 'search.query.executed'?
  local count
  count=$(sqlite3 "$INDEX_DB" "SELECT COUNT(*) FROM messages WHERE source='spine' AND substr(content,1,1)='{' AND json_extract(content, '\$.event') = 'search.query.executed'")
  [ "$count" -eq 0 ]
}

# --- AC: ingester excludes search.query.executed at source ---

@test "chorus-index.sh spine filter skips search.query.executed events" {
  grep -qE "search\.query\.executed" "$INDEXER"
  # The filter must be a skip/continue, not a store — look for the exclusion guard
  grep -qE "(continue|skip|EXCLUDED_EVENTS)" "$INDEXER"
}

@test "chorus-index.sh spine filter also excludes other search.* self-references" {
  # belt-and-suspenders: any event matching search.result.* or similar also excluded
  grep -qE "search\.result|search\.\*" "$INDEXER"
}

# --- AC: live search API returns zero self-referential rows ---

@test "live search 'wren last session shipped friction decisions' returns no spine-source telemetry rows" {
  # Only count results where the SPINE SOURCE emitted the self-referential event.
  # Content matches in session transcripts that discuss the bug by name are
  # legitimate (source=claude or other) — the index isn't eating its tail there.
  local count
  count=$(curl -s "http://localhost:3340/api/chorus/search?q=wren+last+session+shipped+friction+decisions&limit=20" \
    | python3 -c "
import sys, json
d = json.load(sys.stdin)
bad = sum(1 for r in d.get('results', []) if r.get('source') == 'spine' and 'search.query.executed' in r.get('content', ''))
print(bad)
")
  [ "$count" -eq 0 ]
}

@test "live search 'sequence:clearing' returns no spine-source telemetry rows" {
  local count
  count=$(curl -s "http://localhost:3340/api/chorus/search?q=sequence:clearing&limit=20" \
    | python3 -c "
import sys, json
d = json.load(sys.stdin)
bad = sum(1 for r in d.get('results', []) if r.get('source') == 'spine' and 'search.query.executed' in r.get('content', ''))
print(bad)
")
  [ "$count" -eq 0 ]
}

# --- AC: new spine events are filtered on ingest (not just at purge time) ---

@test "post-fix: running indexer does not add rows where event IS search.query.executed" {
  # Check the actual event field, not substring — same reasoning as test 1.
  local before after
  before=$(sqlite3 "$INDEX_DB" "SELECT COUNT(*) FROM messages WHERE source='spine' AND substr(content,1,1)='{' AND json_extract(content, '\$.event') = 'search.query.executed'")
  # Run a search to generate at least one search.query.executed event
  curl -s "http://localhost:3340/api/chorus/search?q=bats-probe-$(date +%s)&limit=1" >/dev/null
  sleep 2
  bash "$INDEXER" spine >/dev/null 2>&1 || true
  sleep 1
  after=$(sqlite3 "$INDEX_DB" "SELECT COUNT(*) FROM messages WHERE source='spine' AND substr(content,1,1)='{' AND json_extract(content, '\$.event') = 'search.query.executed'")
  [ "$after" -eq 0 ]
}
