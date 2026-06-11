#!/usr/bin/env bats
# #1885 — Per-domain error tracking for crawler failures

CRAWLER="$BATS_TEST_DIRNAME/../scripts/index-crawler-snapshots.sh"

# #3019 RCA: this suite used to run the REAL crawler with a fake domain against
# the LIVE status file, LIVE index.db, LIVE api, and LIVE spine. The fake-domain
# entry it wrote into /tmp/crawler-domain-status.json crossed the alert's
# consecutive>=2 threshold — the "intermittent crawler failures, 5x/day for
# WEEKS" on Kade's terminal were THIS SUITE running in nightly/CI/daily-review.
# Hermetic now: every live surface is isolated; emissions are marked synthetic.

setup() {
  export CHORUS_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export STATUS_FILE="$BATS_TEST_TMPDIR/crawler-domain-status.json"
  export CRAWLER_STATUS_FILE="$STATUS_FILE"
  export CRAWLER_DB_PATH="$BATS_TEST_TMPDIR/index.db"
  export CHORUS_SYNTHETIC=1
  # The script DELETEs/INSERTs into messages — give the temp DB the real shape
  # (schema mirrors ~/.chorus/index.db messages table, the script's only write).
  sqlite3 "$CRAWLER_DB_PATH" "CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY AUTOINCREMENT, source TEXT, source_id TEXT, channel TEXT, role TEXT, author TEXT, content TEXT, timestamp TEXT, metadata TEXT)"
}

# Failure paths use the natural 404: a nonexistent domain against the live API
# is a read-only GET. The pollution was never the GET — it was the LIVE status
# file (which the alert reads), LIVE index.db, and unmarked spine events. All
# three are isolated/marked above.

@test "crawler writes per-domain status with timing to status file" {
  # Run crawler for a single known domain
  run bash "$CRAWLER" chorus
  # Status file should exist and contain structured data
  [ -f "$STATUS_FILE" ]
  # Should have chorus domain entry
  run python3 -c "
import json
d = json.load(open('$STATUS_FILE'))
assert 'chorus' in d, 'chorus not in status file'
entry = d['chorus']
assert 'status' in entry, 'no status field'
assert 'duration_ms' in entry, 'no duration_ms field'
assert 'timestamp' in entry, 'no timestamp field'
print('ok')
"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok"* ]]
}

@test "crawler logs failure with error details to status file" {
  # Run crawler for a non-existent domain — should record failure
  run bash "$CRAWLER" nonexistent_fake_domain_xyz
  [ -f "$STATUS_FILE" ]
  run python3 -c "
import json
d = json.load(open('$STATUS_FILE'))
assert 'nonexistent_fake_domain_xyz' in d
entry = d['nonexistent_fake_domain_xyz']
assert entry['status'] == 'error', f'expected error, got {entry[\"status\"]}'
assert entry.get('consecutive_failures', 0) >= 1
print('ok')
"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok"* ]]
}

@test "consecutive failure counter increments across runs" {
  # Run twice for a non-existent domain
  bash "$CRAWLER" nonexistent_fake_domain_xyz 2>/dev/null || true
  bash "$CRAWLER" nonexistent_fake_domain_xyz 2>/dev/null || true
  run python3 -c "
import json
d = json.load(open('$STATUS_FILE'))
entry = d['nonexistent_fake_domain_xyz']
assert entry.get('consecutive_failures', 0) >= 2, f'expected >=2, got {entry.get(\"consecutive_failures\")}'
print('ok')
"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok"* ]]
}

@test "successful crawl resets consecutive failure counter" {
  # First fail, then succeed with a real domain
  bash "$CRAWLER" nonexistent_fake_domain_xyz 2>/dev/null || true
  bash "$CRAWLER" chorus 2>/dev/null || true
  run python3 -c "
import json
d = json.load(open('$STATUS_FILE'))
entry = d['chorus']
assert entry['status'] == 'ok', f'expected ok, got {entry[\"status\"]}'
assert entry.get('consecutive_failures', 0) == 0
print('ok')
"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok"* ]]
}
