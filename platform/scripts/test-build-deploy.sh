#!/bin/bash
# test-build-deploy.sh — BDD tests for /build + /deploy substrate (#2880).
#
# Runs the cucumber feature at platform/tests/features/skills/build-deploy.feature
# against the actual substrate (chorus-log binary, chorus-bin-install,
# chorus-build, git-queue.sh helpers). Same convention as test-demo.sh (#2875).
# Failures here mean the build/deploy contract has drifted — file follow-on cards.
#
# Auto-discovered by nightly-suites.sh via the test-*.sh glob.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TESTS_DIR="$CHORUS_ROOT/platform/tests"
FEATURE="$TESTS_DIR/features/skills/build-deploy.feature"

if [ ! -f "$FEATURE" ]; then
  echo "FAIL: feature file missing at $FEATURE"
  echo "=== Results: 0 passed, 1 failed ==="
  exit 1
fi

if [ ! -d "$TESTS_DIR/node_modules" ]; then
  echo "node_modules missing in $TESTS_DIR — running npm ci"
  (cd "$TESTS_DIR" && npm ci --no-audit --no-fund) >/dev/null 2>&1 || {
    echo "FAIL: npm ci failed"
    echo "=== Results: 0 passed, 1 failed ==="
    exit 1
  }
fi

echo "=== /build + /deploy BDD tests (#2880) ==="
echo ""

CUKE_OUT=$(cd "$TESTS_DIR" && npx --no-install cucumber-js \
  --require-module ts-node/register \
  --require 'features/step_definitions/**/*.ts' \
  --format progress-bar --format summary \
  --tags "@build-deploy-skill and not @wip" \
  "features/skills/build-deploy.feature" 2>&1)
CUKE_RC=$?

echo "$CUKE_OUT"
echo ""

PASSED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+' || echo 0)
FAILED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
PENDING=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ pending' | head -1 | grep -oE '[0-9]+' || echo 0)
UNDEFINED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ undefined' | head -1 | grep -oE '[0-9]+' || echo 0)

TOTAL_FAILED=$((FAILED + PENDING + UNDEFINED))
: "${PASSED:=0}" "${TOTAL_FAILED:=0}"

echo "=== Results: ${PASSED} passed, ${TOTAL_FAILED} failed ==="

if [ "$TOTAL_FAILED" -gt 0 ] || [ "$CUKE_RC" -ne 0 ]; then
  exit 1
fi
exit 0
