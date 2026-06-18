#!/usr/bin/env bash
# #2207 — Detailed shell-level tests for nightly-coverage.sh output format.
# Complements nightly-coverage.bats (existence + exit code) with output
# content checks: percentages, floor values, project names in messages.

set -u
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SCRIPT="${CHORUS_ROOT}/platform/scripts/nightly-coverage.sh"
PASS=0; FAIL=0

p() { PASS=$((PASS+1)); echo "PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "FAIL: $*"; }

TMPFIX=$(mktemp -d)
cleanup() { rm -rf "$TMPFIX"; }
trap cleanup EXIT

FLOORS_FILE="$TMPFIX/floors.yml"
cat > "$FLOORS_FILE" <<'EOF'
ts:
  fake/project-a: 80
  fake/project-b: 90
rust:
  fake/crate-x: 70
EOF

# Green fixtures
mkdir -p "$TMPFIX/green/fake/project-a/coverage"
cat > "$TMPFIX/green/fake/project-a/coverage/coverage-summary.json" <<'EOF'
{"total":{"statements":{"total":100,"covered":85,"pct":85},"branches":{},"functions":{},"lines":{}}}
EOF
mkdir -p "$TMPFIX/green/fake/project-b/coverage"
cat > "$TMPFIX/green/fake/project-b/coverage/coverage-summary.json" <<'EOF'
{"total":{"statements":{"total":100,"covered":95,"pct":95},"branches":{},"functions":{},"lines":{}}}
EOF
mkdir -p "$TMPFIX/green/fake/crate-x"
cat > "$TMPFIX/green/fake/crate-x/llvm-cov-summary.json" <<'EOF'
{"data":[{"totals":{"lines":{"count":200,"covered":150,"percent":75}}}],"type":"llvm.coverage.report","version":"2.0.1"}
EOF

# Regression fixtures: project-b 79% < floor 90
mkdir -p "$TMPFIX/regress/fake/project-a/coverage"
cp "$TMPFIX/green/fake/project-a/coverage/coverage-summary.json" \
   "$TMPFIX/regress/fake/project-a/coverage/coverage-summary.json"
mkdir -p "$TMPFIX/regress/fake/project-b/coverage"
cat > "$TMPFIX/regress/fake/project-b/coverage/coverage-summary.json" <<'EOF'
{"total":{"statements":{"total":100,"covered":79,"pct":79},"branches":{},"functions":{},"lines":{}}}
EOF
mkdir -p "$TMPFIX/regress/fake/crate-x"
cp "$TMPFIX/green/fake/crate-x/llvm-cov-summary.json" \
   "$TMPFIX/regress/fake/crate-x/llvm-cov-summary.json"

run_script() {
  local fixtures="$1"
  NIGHTLY_COVERAGE_FLOORS="$FLOORS_FILE" \
  NIGHTLY_COVERAGE_FIXTURES="$fixtures" \
  NIGHTLY_COVERAGE_DRY_RUN=1 \
  BRIDGE_NUDGE_URL="http://127.0.0.1:3341/nudge" \
  bash "$SCRIPT" 2>&1
}

echo "--- green run output ---"
green=$(run_script "$TMPFIX/green")
rc=$?
[ "$rc" -eq 0 ] && p "exits 0 on green" || f "exits $rc on green"
echo "$green" | grep -qiE "PASS|green|all.*above" && p "contains PASS signal" || f "missing PASS signal; got: $(echo "$green" | head -2)"
echo "$green" | grep -q "85" && p "shows 85% for project-a" || f "missing project-a percentage"
echo "$green" | grep -q "95" && p "shows 95% for project-b" || f "missing project-b percentage"
echo "$green" | grep -q "75" && p "shows 75% for crate-x" || f "missing crate-x percentage"

echo "--- regression run output ---"
regress=$(run_script "$TMPFIX/regress")
rc=$?
[ "$rc" -eq 0 ] && p "exits 0 on regression" || f "exits $rc on regression"
echo "$regress" | grep -qiE "REGRESSION|FAIL|below|floor" && p "has regression signal" || f "missing regression signal"
echo "$regress" | grep -qi "fake/project-b" && p "names failing project" || f "missing failing project name"
echo "$regress" | grep -qiE "floor.*90|90.*floor|floor: 90" && p "shows floor=90" || f "missing floor value"
echo "$regress" | grep -qiE "79|79\.0" && p "shows current 79%" || f "missing current percentage (79)"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
