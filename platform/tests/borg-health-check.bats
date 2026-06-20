#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# Tests for borg-health-check.sh (#2124)
# What Jeff sees: /borg/* pages return 200 even when the data path is broken.
# These tests prove the deep-health probe catches assertion failures, not
# just HTTP 500s.

CHORUS_ROOT="${CHORUS_ROOT}"
SCRIPT="${CHORUS_ROOT}/platform/scripts/borg-health-check.sh"
CONTRACT_DEFAULT="${CHORUS_ROOT}/platform/scripts/borg-health-contract.json"

# Ephemeral port per bats run (low collision odds); teardown kills the pid we
# spawned. No pkill — we only stop processes we started.
export MOCK_PORT="${MOCK_PORT:-$((18000 + RANDOM % 1000))}"
MOCK_DIR=""
MOCK_PID=""

setup() {
  MOCK_DIR=$(mktemp -d)
  mkdir -p "$MOCK_DIR/api/chorus"
}

teardown() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  [ -n "$MOCK_DIR" ] && rm -rf "$MOCK_DIR"
}

start_mock() {
  # exec replaces the subshell with python3, so MOCK_PID is the real python pid
  ( cd "$MOCK_DIR" && exec python3 -m http.server "$MOCK_PORT" >/dev/null 2>&1 ) &
  MOCK_PID=$!
  for _ in $(seq 1 30); do
    curl -sf "http://localhost:${MOCK_PORT}/" >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  echo "mock failed to start on $MOCK_PORT" >&2
  return 1
}

write_fixture() {
  local path="$1"; local body="$2"
  local file="${MOCK_DIR}${path}"
  mkdir -p "$(dirname "$file")"
  printf '%s' "$body" > "$file"
}

run_probe() {
  local contract="$1"
  BORG_HEALTH_API_BASE="http://localhost:${MOCK_PORT}" \
    BORG_HEALTH_CONTRACT="$contract" \
    bash "$SCRIPT"
}

@test "script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "default contract is valid JSON with probes array" {
  run python3 -c "import json; d=json.load(open('$CONTRACT_DEFAULT')); assert isinstance(d['probes'], list) and len(d['probes']) > 0"
  [ "$status" -eq 0 ]
}

@test "passes when all JSON assertions hold" {
  start_mock
  write_fixture "/api/chorus/foo" '{"total": 5}'
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/foo/","api":"/api/chorus/foo","assert":"d.get(\"total\", 0) > 0"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS /borg/foo/"* ]]
  [[ "$output" == *"probes passed"* ]]
}

@test "fails when backing API returns non-200" {
  start_mock
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/missing/","api":"/api/chorus/missing","assert":"True"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL /borg/missing/"* ]]
  [[ "$output" == *"returned 404"* ]]
}

@test "fails when JSON assertion evaluates false" {
  start_mock
  write_fixture "/api/chorus/cost" '{"summary": {"totalCost": 0}}'
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/cost/","api":"/api/chorus/cost","assert":"float(d[\"summary\"][\"totalCost\"]) > 0"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL /borg/cost/"* ]]
  [[ "$output" == *"assertion false"* ]]
}

@test "fails when assertion raises an exception" {
  start_mock
  write_fixture "/api/chorus/broken" '{}'
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/broken/","api":"/api/chorus/broken","assert":"d[\"missing_key\"] > 0"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL /borg/broken/"* ]]
  [[ "$output" == *"assertion raised"* ]]
}

@test "content-type probe passes for image response" {
  start_mock
  mkdir -p "${MOCK_DIR}/api/chorus/jeff/posture"
  printf 'fake-png' > "${MOCK_DIR}/api/chorus/jeff/posture/strip.png"
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/jeff/","api":"/api/chorus/jeff/posture/strip.png","assert_content_type":"image/"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS /borg/jeff/"* ]]
}

@test "content-type probe fails when Content-Type is wrong" {
  start_mock
  write_fixture "/api/chorus/text" 'hello world'
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/text/","api":"/api/chorus/text","assert_content_type":"image/"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL /borg/text/"* ]]
}

@test "aggregates multiple probe failures and reports count" {
  start_mock
  write_fixture "/api/chorus/good" '{"n": 1}'
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[
  {"page":"/borg/good/","api":"/api/chorus/good","assert":"d[\"n\"] > 0"},
  {"page":"/borg/bad/","api":"/api/chorus/bad","assert":"True"}
]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"PASS /borg/good/"* ]]
  [[ "$output" == *"FAIL /borg/bad/"* ]]
  [[ "$output" == *"1/2 probe(s) failed"* ]]
}

# --- Kade's error-path coverage ---

@test "exits with error when contract file is missing" {
  local contract="${MOCK_DIR}/does-not-exist.json"
  run env BORG_HEALTH_CONTRACT="$contract" bash "$SCRIPT"
  [ "$status" -eq 2 ]
  [[ "$output" == *"contract missing"* ]]
}

@test "exits with error when contract is malformed JSON" {
  start_mock
  local contract="${MOCK_DIR}/contract.json"
  printf '{this is not valid json' > "$contract"
  run run_probe "$contract"
  [ "$status" -ne 0 ]
  [[ "$output" == *"JSONDecodeError"* || "$output" == *"Expecting"* || "$output" == *"error"* ]]
}

@test "exits cleanly when probes array is empty" {
  start_mock
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"probes":[]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 0 ]
  [[ "$output" == *"0/0 probes passed"* ]]
}

@test "fails probe when entry is missing both assert and assert_content_type" {
  start_mock
  write_fixture "/api/chorus/noop" '{"ok": true}'
  local contract="${MOCK_DIR}/contract.json"
  # page + api present, no assertion of either kind — treated as malformed probe
  cat > "$contract" <<'JSON'
{"probes":[{"page":"/borg/noop/","api":"/api/chorus/noop"}]}
JSON
  run run_probe "$contract"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL /borg/noop/"* ]]
}

@test "fails probe when contract is missing 'probes' key" {
  start_mock
  local contract="${MOCK_DIR}/contract.json"
  cat > "$contract" <<'JSON'
{"other_key":[]}
JSON
  run run_probe "$contract"
  [ "$status" -ne 0 ]
  [[ "$output" == *"KeyError"* || "$output" == *"probes"* || "$output" == *"error"* ]]
}
