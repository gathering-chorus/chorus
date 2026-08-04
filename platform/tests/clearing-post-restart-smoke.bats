#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# clearing-post-restart-smoke.bats — Integration test against real localhost (#2333).
#
# Validator branches are unit-tested in clearing_flow_shape_validator_test.py.
# This file proves the shell wrapper wires curl + validator + spine event correctly
# against the actual Clearing service.

# #3606 — repointed out of another role's werk. This hardcoded
# chorus-werk/silas/platform/scripts/... — a canonical test reaching into SILAS'S
# WERK, and specifically the PERSISTENT per-role werk that #2913 retired. Once
# that directory went away the test asserted `[ -x "$SCRIPT" ]` against a path
# that cannot exist. The script has been in canonical the whole time.
SCRIPT="${CHORUS_ROOT}/platform/scripts/clearing-post-restart-smoke.sh"

@test "smoke script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "smoke passes against running localhost:3470" {
  if ! curl -sf --max-time 2 http://localhost:3470/health -o /dev/null; then
    skip "Clearing not running on localhost:3470"
  fi
  TMPDIR=$(mktemp -d)
  STUB="$TMPDIR/log-stub.sh"
  cat > "$STUB" <<EOF
#!/bin/bash
echo "\$@" >> "$TMPDIR/events"
EOF
  chmod +x "$STUB"

  CHORUS_LOG_BIN="$STUB" SMOKE_LOG="$TMPDIR/smoke.log" HEALTH_TIMEOUT=5 \
    run bash "$SCRIPT"
  echo "stdout: $output"
  echo "events:"; cat "$TMPDIR/events" 2>/dev/null || echo "(none)"
  [ "$status" -eq 0 ]
  grep -q "clearing.smoke.passed" "$TMPDIR/events"
  rm -rf "$TMPDIR"
}
