#!/usr/bin/env bats
# @test-type: integration
# @domain: identity — guards #4787 (Slice 4 of #4780): verified caller identity, narrow ops helper, named service events

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$ROOT"
  export CHORUS_LOG_FILE="$T/chorus.log"
  export LOKI_FIRST_OVERRIDE=1
}

# --- AC3: service.started and service.stopped spine events name the service ---

@test "AC3: service.started and service.stopped spine events name the service" {
  LABEL="com.chorus.test-probe"
  bash "$ROOT/platform/scripts/service-run" "$LABEL" job /usr/bin/true

  # Assert service.started exists and carries "service":"$LABEL"
  start_line="$(grep '"event":"service.started"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$start_line"
  [[ "$start_line" =~ \"service\":\"$LABEL\" ]] || return 1

  # Assert service.stopped exists and carries "service":"$LABEL"
  stop_line="$(grep '"event":"service.stopped"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$stop_line"
  [[ "$stop_line" =~ \"service\":\"$LABEL\" ]] || return 1
}

@test "AC3 NEGATIVE PROOF (#3734): an event missing 'service' field fails verification" {
  # Verification predicate: line must have "service":"com.chorus.*"
  verify_service_named() {
    local line="$1"
    [[ "$line" =~ \"service\":\"com\.chorus\.[a-zA-Z0-9_-]+\" ]] || return 1
  }

  # Valid line passes
  valid_line='{"event":"service.started","service":"com.chorus.test-probe","pid":"1234"}'
  verify_service_named "$valid_line"

  # Missing service field fails
  missing_service='{"event":"service.started","pid":"1234"}'
  ! verify_service_named "$missing_service" || return 1

  # Empty service string fails
  empty_service='{"event":"service.started","service":"","pid":"1234"}'
  ! verify_service_named "$empty_service" || return 1
}
