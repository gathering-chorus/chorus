#!/bin/bash
# test-demo.sh — BDD tests for /demo skill (#2875).
#
# Runs the cucumber feature file at platform/tests/features/skills/demo.feature
# against the actual /demo execution surfaces (cards CLI, chorus-log, messaging
# API, smoke-check.sh). Failures here indicate /demo's contract has drifted
# from what the substrate enforces — file follow-on cards for each gap.
#
# Auto-discovered by nightly-suites.sh via the test-*.sh glob.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TESTS_DIR="$CHORUS_ROOT/platform/tests"
FEATURE="$TESTS_DIR/features/skills/demo.feature"

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

echo "=== /demo BDD tests (#2875) ==="
echo ""

# Run cucumber against just the demo feature. Override the default config's
# `paths` with explicit feature path + tag filter (default config globs all
# features, which would pull in unrelated suites).
CUKE_OUT=$(cd "$TESTS_DIR" && npx --no-install cucumber-js \
  --require-module ts-node/register \
  --require 'features/step_definitions/**/*.ts' \
  --format progress-bar --format summary \
  --tags "@demo-skill and not @wip" \
  "features/skills/demo.feature" 2>&1)
CUKE_RC=$?

echo "$CUKE_OUT"
echo ""

# Parse cucumber summary line: "N scenarios (X passed, Y failed, Z pending)"
PASSED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+' || echo 0)
FAILED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
PENDING=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ pending' | head -1 | grep -oE '[0-9]+' || echo 0)
UNDEFINED=$(echo "$CUKE_OUT" | grep -oE '[0-9]+ undefined' | head -1 | grep -oE '[0-9]+' || echo 0)

# Treat pending + undefined as failures for the gate (they're gaps).
TOTAL_FAILED=$((FAILED + PENDING + UNDEFINED))
: "${PASSED:=0}" "${TOTAL_FAILED:=0}"

# Canonical summary line per #2856 contract (parsed by nightly-suites.sh).
echo "=== Results: ${PASSED} passed, ${TOTAL_FAILED} failed ==="

# Exit non-zero if anything failed or cucumber itself errored.
if [ "$TOTAL_FAILED" -gt 0 ] || [ "$CUKE_RC" -ne 0 ]; then
  exit 1
fi
exit 0
