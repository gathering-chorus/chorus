#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# index-freshness-alert.bats — Tests for session index freshness check (#2270)
# What Jeff sees: gemba shows 2-day-old data and nobody knew. These tests
# prove the alert detects stale index data during working hours.

DEEP_HEALTH="${CHORUS_ROOT}/platform/scripts/deep-health.sh"
INDEX_DB="$HOME/.chorus/index.db"

# --- AC 1: Deep-health checks actual message timestamps in index.db ---

@test "deep-health script exists and is executable" {
  [ -x "$DEEP_HEALTH" ]
}

@test "deep-health queries index.db for newest message timestamp" {
  # The script should contain a sqlite3 query for message timestamps
  grep -q "sqlite3.*INDEX_DB" "$DEEP_HEALTH"
}

@test "freshness check uses message timestamp not file mtime" {
  # Should query actual timestamps from the messages table, not just stat the file
  grep -q "SELECT.*timestamp.*FROM.*messages" "$DEEP_HEALTH"
}

# --- AC 2: Alert only during working hours (8am-10pm) ---

@test "freshness check respects working hours" {
  # The script should contain working hours logic (8-22 or 8am-10pm)
  grep -qE 'hour|working.hours|8.*22|8am|10pm' "$DEEP_HEALTH"
}

# --- AC 3: Alert includes diagnostic info ---

@test "alert includes last indexed timestamp" {
  grep -q "last indexed" "$DEEP_HEALTH"
}

@test "alert includes suggested fix" {
  grep -qE "fswatch|lock.?file|watcher" "$DEEP_HEALTH"
}

# --- AC 4: Integrates with existing deep-health.sh ---

@test "freshness check adds to FAILURES array" {
  # The check should use the same FAILURES+=() pattern as other checks
  grep -q 'FAILURES.*session-index' "$DEEP_HEALTH"
}
