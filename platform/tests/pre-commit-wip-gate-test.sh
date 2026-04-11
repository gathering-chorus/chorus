#!/usr/bin/env bash
# Test: pre-commit WIP gate — reject commits when role has no WIP card (#1799)
# RED before gate exists. GREEN after.
set -euo pipefail

SCAN_DIR="/tmp/claude-team-scan"
PASS=0
FAIL=0

# The gate logic extracted for testability.
# In production this runs inside .git/hooks/pre-commit.
wip_gate_check() {
  local role="${DEPLOY_ROLE:-}"
  local msg="${COMMIT_MSG:-}"

  # No role = not a role commit, bypass
  [ -z "$role" ] && return 0

  # Swat/chore bypass
  echo "$msg" | grep -qiE '\b(swat|chore)\b' && return 0

  # Check for WIP card in role state
  local state_file="${SCAN_DIR}/${role}-declared.json"
  if [ ! -f "$state_file" ]; then
    echo "pre-commit: blocked — ${role} has no WIP card declared. Pull a card first."
    return 1
  fi

  local state card
  state=$(python3 -c "import json; d=json.load(open('${state_file}')); print(d.get('state',''))" 2>/dev/null || echo "")
  card=$(python3 -c "import json; d=json.load(open('${state_file}')); print(d.get('card',''))" 2>/dev/null || echo "")

  if [ "$state" != "building" ] || [ -z "$card" ] || [ "$card" = "None" ]; then
    echo "pre-commit: blocked — ${role} has no WIP card declared. Pull a card first."
    return 1
  fi

  return 0
}

run_test() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

run_test_expect_fail() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  FAIL: $name (expected failure but got success)"
    FAIL=$((FAIL + 1))
  else
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  fi
}

echo "=== pre-commit WIP gate tests ==="

mkdir -p "$SCAN_DIR"

# 1. No DEPLOY_ROLE — bypass (Jeff's direct commits)
run_test "bypass when DEPLOY_ROLE unset" env -u DEPLOY_ROLE bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; wip_gate_check"

# 2. Role with no state file — blocked
run_test_expect_fail "blocked when no state file" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; export DEPLOY_ROLE; wip_gate_check"

# 3. Role with WIP card — allowed
echo '{"role":"testrol","state":"building","card":1799}' > "$SCAN_DIR/testrol-declared.json"
run_test "allowed when role has WIP card" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; export DEPLOY_ROLE; wip_gate_check"

# 4. Role idle (no card) — blocked
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test_expect_fail "blocked when role is idle" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; export DEPLOY_ROLE; wip_gate_check"

# 5. Swat bypass — allowed even without card
echo '{"role":"testrol","state":"idle"}' > "$SCAN_DIR/testrol-declared.json"
run_test "bypass for swat commit" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; COMMIT_MSG='silas: swat — fix something'; export DEPLOY_ROLE COMMIT_MSG; wip_gate_check"

# 6. Chore bypass — allowed even without card
run_test "bypass for chore commit" bash -c "$(declare -f wip_gate_check); SCAN_DIR='$SCAN_DIR'; DEPLOY_ROLE=testrol; COMMIT_MSG='silas: chore — cleanup'; export DEPLOY_ROLE COMMIT_MSG; wip_gate_check"

# Cleanup test state
rm -f "$SCAN_DIR/testrol-declared.json"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
