#!/usr/bin/env bats
# @test-type: integration — hermetic. A python fixture door on an ephemeral
# loopback port stands in for the generated API; no live store, no live server.
#
# #4105 — the registered-vs-executed census. Two states it could not tell
# apart before this card:
#   1. "the ledger is exhausted" vs "we stopped at the page cap" — the walk
#      broke on both, so a ledger longer than the cap reported executed tests
#      as never-run.
#   2. a page that fails MID-walk. #4022 fixed page 1 (a 502 read as an empty
#      ledger, "7,794 never ran"); pages 2..n kept the old behaviour until now.
# Negative proofs (#3734): each violating state is shown to produce UNMEASURED,
# and the control — a ledger read whole — still reports its real gap.

setup() {
  ROOT="$BATS_TEST_DIRNAME/../.."
  SERVER="$BATS_TEST_DIRNAME/fixtures/census-ledger-server.py"
  BIN="${WERK_TEST_BIN:-$ROOT/platform/services/werk-test/target/release/werk-test}"
  [ -x "$BIN" ] || BIN="$ROOT/target/release/werk-test"
  [ -x "$BIN" ] || skip "werk-test binary not built"
  export CHORUS_LOG_BIN=/dev/null
}

teardown() {
  [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null
  return 0
}

# start the fixture door, export the two endpoints the census reads
start_door() {
  local port_file="$BATS_TEST_TMPDIR/port"
  python3 "$SERVER" > "$port_file" 2>/dev/null &
  SRV_PID=$!
  disown 2>/dev/null || true
  local i=0
  while [ ! -s "$port_file" ] && [ $i -lt 100 ]; do sleep 0.05; i=$((i+1)); done
  PORT=$(cat "$port_file")
  [ -n "$PORT" ] || return 1
  export OWL_API_TESTS="http://127.0.0.1:$PORT/tests?limit=10000"
  export OWL_API_TESTRESULTS="http://127.0.0.1:$PORT/testresults?limit=${PAGE_SIZE:-2}"
}

@test "negative proof: a ledger longer than the page cap is UNMEASURED, never a never-run gap" {
  export LEDGER_ROWS=10 REGISTERED=10 PAGE_SIZE=2
  export CENSUS_PAGE_CAP=2
  start_door
  run "$BIN" --reconcile
  [ "$status" -ne 0 ]
  [[ "$output" == *"truncated"* ]]
  [[ "$output" == *"2 pages"* ]]
  # the whole point: no gap number is computed from a partial read
  [[ "$output" != *"never-run ("* ]]
}

@test "control: the same ledger read whole cross-foots — no gap, exit 0" {
  export LEDGER_ROWS=10 REGISTERED=10 PAGE_SIZE=2
  export CENSUS_PAGE_CAP=50
  start_door
  run "$BIN" --reconcile
  [ "$status" -eq 0 ]
  [[ "$output" == *"never-run: none"* ]]
  [[ "$output" != *"truncated"* ]]
}

@test "control: a real gap in a fully-read ledger is still reported" {
  export LEDGER_ROWS=6 REGISTERED=10 PAGE_SIZE=2
  export CENSUS_PAGE_CAP=50
  start_door
  run "$BIN" --reconcile
  [ "$status" -eq 0 ]
  [[ "$output" == *"never-run (4)"* ]]
}

# Regression guard, not a new proof: #4022 already made a failed fetch an
# error rather than an empty ledger, and it already held mid-walk. This pins
# that behaviour to a fixture so the page-2 case cannot silently regress.
@test "regression guard: a 502 on page 2 is UNMEASURED naming the page, never an empty ledger" {
  export LEDGER_ROWS=10 REGISTERED=10 PAGE_SIZE=2 FAIL_PAGE=2
  export CENSUS_PAGE_CAP=50
  start_door
  run "$BIN" --reconcile
  [ "$status" -ne 0 ]
  [[ "$output" == *"page 2"* ]]
  [[ "$output" != *"never-run ("* ]]
}

@test "the census says how many pages it read" {
  export LEDGER_ROWS=10 REGISTERED=10 PAGE_SIZE=2
  export CENSUS_PAGE_CAP=50
  start_door
  run "$BIN" --reconcile
  [ "$status" -eq 0 ]
  [[ "$output" == *"5 pages"* ]]
}

@test "the nightly leg turns a truncated census into UNMEASURED, not red" {
  stub="$BATS_TEST_TMPDIR/werk-test"
  printf '#!/usr/bin/env bash\necho "census UNMEASURED: walk truncated at 2 pages"\nexit 1\n' > "$stub"
  chmod +x "$stub"
  NIGHTLY_RECONCILE_BIN="$stub" run bash -c "source '$ROOT/platform/scripts/nightly-suites.sh'; _reconcile_leg"
  [[ "$output" == *"|unmeasured|"* ]]
  [[ "$output" == *"truncated"* ]]
  [[ "$output" != *"|fail|"* ]]
}
