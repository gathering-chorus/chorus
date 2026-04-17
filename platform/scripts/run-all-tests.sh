#!/bin/bash
# run-all-tests.sh — unified chorus test runner (#2156)
#
# Runs every test suite in the chorus monorepo with a single invocation
# and emits a per-suite line + aggregate summary. Mirrors gathering's
# test:all script conceptually; doesn't claim parity yet, but creates
# the foundation we need before coverage gates and CI land.
#
# Usage:
#   run-all-tests.sh                   # run all suites, hermetic default
#   run-all-tests.sh --integration     # ALSO run non-hermetic integration tests
#   run-all-tests.sh --only rust|ts|shell  # filter to one layer
#   run-all-tests.sh --json            # emit JSON summary for nightly consumption
#   run-all-tests.sh --coverage        # run with coverage (tarpaulin/jest), emit spine events
#
# Exit: 0 if every suite passed, 1 if any failed, 2 on runner error.

set -u

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
cd "$CHORUS_ROOT"

# Parse args (while+shift so --only rust and --only=rust both work)
INTEGRATION_MODE=0
ONLY=""
JSON_MODE=0
COVERAGE_MODE=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --integration) INTEGRATION_MODE=1 ;;
    --only) shift; ONLY="$1" ;;
    --only=*) ONLY="${1#--only=}" ;;
    --json) JSON_MODE=1 ;;
    --coverage) COVERAGE_MODE=1 ;;
    -h|--help)
      sed -n '3,16p' "$0" | sed 's/^# //; s/^#//'
      exit 0 ;;
  esac
  shift
done

# HERMETIC_TEST_MODE default ON unless --integration
if [ "$INTEGRATION_MODE" = "0" ]; then
  export HERMETIC_TEST_MODE=1
fi

# Output style
if [ "$JSON_MODE" = "1" ]; then
  SUITE_MARKER="SUITE"
  RED='' GREEN='' YELLOW='' NC=''
else
  SUITE_MARKER=""
  RED='\033[0;31m' GREEN='\033[0;32m' YELLOW='\033[1;33m' NC='\033[0m'
fi

declare -i PASS=0
declare -i FAIL=0
declare -i SKIP=0
FAILED_SUITES=""

record() {
  local kind="$1" name="$2" status="$3" summary="$4"
  local color="$GREEN" label="PASS"
  case "$status" in
    fail) color="$RED"; label="FAIL"; FAIL=$((FAIL+1)); FAILED_SUITES+="$kind:$name " ;;
    skip) color="$YELLOW"; label="SKIP"; SKIP=$((SKIP+1)) ;;
    *)    PASS=$((PASS+1)) ;;
  esac
  if [ "$JSON_MODE" = "1" ]; then
    printf "SUITE|%s|%s|%s|%s\n" "$kind" "$name" "$status" "$summary"
  else
    printf "  ${color}%-4s${NC}  %-8s  %-50s  %s\n" "$label" "$kind" "$name" "$summary"
  fi
}

CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

# Emit coverage.measured spine event when in coverage mode so Borg can trend.
emit_coverage() {
  local kind="$1" svc="$2" pct="$3"
  [ "$COVERAGE_MODE" = "1" ] || return 0
  [ -x "$CHORUS_LOG" ] || return 0
  "$CHORUS_LOG" coverage.measured kade "kind=$kind service=$svc coverage=$pct" 2>/dev/null || true
}

run_rust() {
  local svc="$1"
  local dir="$CHORUS_ROOT/platform/services/$svc"
  [ -f "$dir/Cargo.toml" ] || return 0
  local out rc
  if [ "$COVERAGE_MODE" = "1" ] && command -v cargo-tarpaulin >/dev/null 2>&1; then
    out=$(cd "$dir" && cargo tarpaulin --skip-clean 2>&1)
    rc=$?
    # Tarpaulin line: "NN.NN% coverage, X/Y lines covered"
    local pct
    pct=$(echo "$out" | grep -oE '[0-9]+\.[0-9]+% coverage' | head -1 | sed 's/% coverage//')
    [ -n "$pct" ] && emit_coverage rust "$svc" "$pct"
    local summary="coverage=${pct:-n/a}%"
    if [ $rc -eq 0 ]; then
      record rust "$svc" pass "$summary"
    else
      record rust "$svc" fail "$summary (floor breach or test fail)"
    fi
    return
  fi
  out=$(cd "$dir" && cargo test --release 2>&1)
  rc=$?
  local summary
  summary=$(echo "$out" | grep -E '^test result:' | tail -1 | sed 's/test result: //')
  [ -z "$summary" ] && summary="no output"
  if [ $rc -eq 0 ]; then
    record rust "$svc" pass "$summary"
  else
    record rust "$svc" fail "$summary"
  fi
}

run_ts() {
  local dir="$1"
  [ -f "$dir/package.json" ] || return 0
  grep -q '"test"' "$dir/package.json" || return 0
  local name
  name=$(basename "$dir")
  local out rc test_cmd
  if [ "$COVERAGE_MODE" = "1" ] && grep -q '"test:coverage"' "$dir/package.json"; then
    test_cmd="npm run test:coverage --silent"
  else
    test_cmd="npm test --silent"
  fi
  out=$(cd "$dir" && $test_cmd 2>&1)
  rc=$?
  local summary
  summary=$(echo "$out" | grep -E 'Tests:' | tail -1)
  if [ "$COVERAGE_MODE" = "1" ]; then
    # Jest table footer: "All files | NN | NN | NN | NN |"
    local line_pct
    line_pct=$(echo "$out" | grep -E '^All files' | awk -F'|' '{gsub(/ /,"",$5); print $5}' | head -1)
    [ -n "$line_pct" ] && emit_coverage ts "$name" "$line_pct"
    summary="${summary:-exit=$rc} | lines=${line_pct:-n/a}%"
  fi
  [ -z "$summary" ] && summary="exit=$rc"
  if [ $rc -eq 0 ]; then
    record ts "$name" pass "$summary"
  else
    record ts "$name" fail "$summary"
  fi
}

run_bats() {
  local f="$1"
  [ -f "$f" ] || return 0
  local name
  name=$(basename "$f" .test.sh)
  local out
  out=$(bash "$f" 2>&1)
  local rc=$?
  local summary
  summary=$(echo "$out" | grep -E 'passed|Results:' | tail -1)
  [ -z "$summary" ] && summary="exit=$rc"
  if [ $rc -eq 0 ]; then
    record bash "$name" pass "$summary"
  else
    record bash "$name" fail "$summary"
  fi
}

[ "$JSON_MODE" = "0" ] && echo "== Chorus test suite (hermetic=$HERMETIC_TEST_MODE integration=$INTEGRATION_MODE coverage=$COVERAGE_MODE) =="

if [ -z "$ONLY" ] || [ "$ONLY" = "rust" ]; then
  [ "$JSON_MODE" = "0" ] && echo "-- Rust --"
  run_rust chorus-hooks
  run_rust chorus-inject
fi

if [ -z "$ONLY" ] || [ "$ONLY" = "ts" ]; then
  [ "$JSON_MODE" = "0" ] && echo "-- TypeScript --"
  for dir in \
    "$CHORUS_ROOT/directing/products/cards" \
    "$CHORUS_ROOT/directing/clearing" \
    "$CHORUS_ROOT/platform/workflow-engine" \
    "$CHORUS_ROOT/platform/tests" \
    "$CHORUS_ROOT/platform/chorus-sdk" \
    "$CHORUS_ROOT/roles/wren/scripts"; do
    run_ts "$dir"
  done
fi

if [ -z "$ONLY" ] || [ "$ONLY" = "shell" ]; then
  [ "$JSON_MODE" = "0" ] && echo "-- Shell --"
  for f in "$CHORUS_ROOT"/platform/tests/*.test.sh; do
    run_bats "$f"
  done
fi

# Summary
if [ "$JSON_MODE" = "1" ]; then
  printf '{"pass":%d,"fail":%d,"skip":%d,"failed_suites":"%s"}\n' "$PASS" "$FAIL" "$SKIP" "$FAILED_SUITES"
else
  echo ""
  echo "== Summary =="
  printf "  ${GREEN}pass:${NC} %d   ${RED}fail:${NC} %d   ${YELLOW}skip:${NC} %d\n" "$PASS" "$FAIL" "$SKIP"
  if [ -n "$FAILED_SUITES" ]; then
    echo "  failed: $FAILED_SUITES"
  fi
fi

[ "$FAIL" -gt 0 ] && exit 1
exit 0
