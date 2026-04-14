#!/usr/bin/env bats
# #1885 — Per-domain error tracking for crawler failures

CRAWLER="$BATS_TEST_DIRNAME/../scripts/index-crawler-snapshots.sh"
STATUS_FILE="/tmp/crawler-domain-status.json"

setup() {
  export CHORUS_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  # Clean status file before each test
  rm -f "$STATUS_FILE"
}

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
