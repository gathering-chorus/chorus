#!/bin/bash
# test-acp.sh — BDD tests for /acp skill (#2879).
#
# Sibling to test-demo.sh. Runs the cucumber feature file at
# platform/tests/features/skills/acp.feature against the actual /acp
# execution surfaces (chorus_acp service path, git-queue.sh, gh CLI
# stubbed via the test-fixture binary, cards CLI, chorus-log).
#
# Auto-discovered by nightly-suites.sh via the test-*.sh glob.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TESTS_DIR="$CHORUS_ROOT/platform/tests"
FEATURE="$TESTS_DIR/features/skills/acp.feature"
GH_STUB_DIR="$TESTS_DIR/fixtures/gh-stub"
CARDS_STUB_DIR="$TESTS_DIR/fixtures/cards-stub"

if [ ! -f "$FEATURE" ]; then
  echo "FAIL: feature file missing at $FEATURE"
  echo "=== Results: 0 passed, 1 failed ==="
  exit 1
fi

if [ ! -x "$GH_STUB_DIR/gh" ]; then
  echo "FAIL: gh stub missing at $GH_STUB_DIR/gh"
  echo "=== Results: 0 passed, 1 failed ==="
  exit 1
fi

if [ ! -x "$CARDS_STUB_DIR/cards" ]; then
  echo "FAIL: cards stub missing at $CARDS_STUB_DIR/cards"
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

echo "=== /acp BDD tests (#2879) ==="
echo ""

# Hermetic state per run: gh stub state, fixture origin paths, trace marker.
RUN_DIR=$(mktemp -d -t acp-bdd-XXXXXX)
export GH_STUB_STATE="$RUN_DIR/gh-stub-state.json"
export ACP_BDD_RUN_DIR="$RUN_DIR"
# Prepend gh stub + cards stub to PATH so chorus_acp / git-queue invocations
# hit them instead of system gh / canonical cards.
export PATH="$GH_STUB_DIR:$CARDS_STUB_DIR:$PATH"

cleanup() {
  # Keep run-dir on failure for postmortem; clean on success.
  if [ "${KEEP_RUN_DIR:-0}" != "1" ] && [ "${SUITE_RC:-1}" -eq 0 ]; then
    rm -rf "$RUN_DIR"
  else
    [ -d "$RUN_DIR" ] && echo "Run dir kept at: $RUN_DIR"
  fi
}
trap cleanup EXIT

CUKE_OUT=$(cd "$TESTS_DIR" && npx --no-install cucumber-js \
  --require-module ts-node/register \
  --require 'features/step_definitions/**/*.ts' \
  --format progress-bar --format summary \
  --tags "@acp-skill and not @wip" \
  "features/skills/acp.feature" 2>&1)
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

SUITE_RC=0
if [ "$TOTAL_FAILED" -gt 0 ] || [ "$CUKE_RC" -ne 0 ]; then
  SUITE_RC=1
fi
exit $SUITE_RC
