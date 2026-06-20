#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# fuseki-harvest-stale-alert.bats — #2327
# What Jeff sees: a fuseki-harvest-stale nudge only when photos really are absent,
# never when Fuseki blipped for a moment. These tests prove the three failure modes
# (curl error, count=0, count>0) produce three distinct outcomes.

CHECK_SCRIPT="${CHORUS_ROOT}/proving/domains/alerts/fuseki-harvest-stale.check.sh"
ALERT_YAML="${CHORUS_ROOT}/proving/domains/alerts/fuseki-harvest-stale.yml"

setup() {
  export TEST_TMP="$(mktemp -d)"
  export PATH_BACKUP="$PATH"
}

teardown() {
  rm -rf "$TEST_TMP"
  export PATH="$PATH_BACKUP"
}

# --- AC: check script exists, executable, env-overridable ---

@test "check script exists and is executable" {
  [ -x "$CHECK_SCRIPT" ]
}

@test "alert yaml references the check script" {
  grep -qF "fuseki-harvest-stale.check.sh" "$ALERT_YAML"
}

# --- AC: distinguishes curl failure from query success ---

@test "curl failure (non-zero exit) exits 0 with warn marker, does NOT fire alert" {
  cat > "$TEST_TMP/curl" <<'EOF'
#!/bin/bash
# Simulate curl network failure
exit 7
EOF
  chmod +x "$TEST_TMP/curl"
  export PATH="$TEST_TMP:$PATH"

  run "$CHECK_SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"unreachable"* || "$output" == *"skip"* || "$output" == *"warn"* ]]
}

@test "curl success with count=0 fires alert (exit 1)" {
  cat > "$TEST_TMP/curl" <<'EOF'
#!/bin/bash
cat <<'JSON'
{"head":{"vars":["c"]},"results":{"bindings":[{"c":{"type":"literal","datatype":"http://www.w3.org/2001/XMLSchema#integer","value":"0"}}]}}
JSON
exit 0
EOF
  chmod +x "$TEST_TMP/curl"
  export PATH="$TEST_TMP:$PATH"

  run "$CHECK_SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"empty"* || "$output" == *"fuseki-harvest"* ]]
}

@test "curl success with count>0 is ok (exit 0)" {
  cat > "$TEST_TMP/curl" <<'EOF'
#!/bin/bash
cat <<'JSON'
{"head":{"vars":["c"]},"results":{"bindings":[{"c":{"type":"literal","datatype":"http://www.w3.org/2001/XMLSchema#integer","value":"38074"}}]}}
JSON
exit 0
EOF
  chmod +x "$TEST_TMP/curl"
  export PATH="$TEST_TMP:$PATH"

  run "$CHECK_SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok"* ]]
  [[ "$output" == *"38074"* ]]
}

# --- AC: skip-count is observable (counter/log when skipping) ---

@test "curl failure path writes a skip marker we can observe" {
  cat > "$TEST_TMP/curl" <<'EOF'
#!/bin/bash
exit 7
EOF
  chmod +x "$TEST_TMP/curl"
  export PATH="$TEST_TMP:$PATH"
  export FUSEKI_SKIP_LOG="$TEST_TMP/skip.log"

  run "$CHECK_SCRIPT"
  [ "$status" -eq 0 ]
  [ -s "$FUSEKI_SKIP_LOG" ]
}
