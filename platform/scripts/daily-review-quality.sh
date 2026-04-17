#!/bin/bash
# daily-review-quality.sh — 6am quality check, posts to Bridge
# Card #1766 | DEC-107 compliant (no osascript)
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_LOG="$SCRIPT_DIR/chorus-log"
TIMESTAMP=$(TZ=America/New_York date '+%Y-%m-%d %H:%M')
APP_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"
STATUS="green"
ISSUES=""

# --- Smoke tests ---
SMOKE_OUTPUT=$(bash ${CHORUS_ROOT}/platform/scripts/smoke-check.sh --all 2>&1 || true)
SMOKE_PASS=$(echo "$SMOKE_OUTPUT" | grep -c "PASS" || true)
SMOKE_PASS=${SMOKE_PASS:-0}
SMOKE_FAIL=$(echo "$SMOKE_OUTPUT" | grep -c "FAIL" || true)
SMOKE_FAIL=${SMOKE_FAIL:-0}
if [ "$SMOKE_FAIL" -gt 0 ] 2>/dev/null; then
  STATUS="red"
  ISSUES+="**Smoke tests:** ${SMOKE_PASS} pass, ${SMOKE_FAIL} fail\n"
  ISSUES+="$(echo "$SMOKE_OUTPUT" | grep "FAIL" | head -5 | sed 's/^/  /')\n"
fi

# --- Lint ---
LINT_OUTPUT=$(cd "$APP_DIR" && npx eslint src/ --max-warnings 999 --format compact 2>&1 | tail -1 || true)
LINT_WARNINGS=$(echo "$LINT_OUTPUT" | grep -oE '[0-9]+ warning' | grep -oE '[0-9]+' || true)
LINT_WARNINGS=${LINT_WARNINGS:-0}
LINT_ERRORS=$(echo "$LINT_OUTPUT" | grep -oE '[0-9]+ error' | grep -oE '[0-9]+' || true)
LINT_ERRORS=${LINT_ERRORS:-0}
if [ "$LINT_ERRORS" -gt 0 ] 2>/dev/null; then
  STATUS="red"
  ISSUES+="**Lint:** ${LINT_ERRORS} errors, ${LINT_WARNINGS} warnings\n"
elif [ "$LINT_WARNINGS" -gt 10 ] 2>/dev/null; then
  [ "$STATUS" = "green" ] && STATUS="yellow"
  ISSUES+="**Lint:** ${LINT_WARNINGS} warnings (ceiling: 10)\n"
fi

# --- Test suite ---
TEST_OUTPUT=$(cd "$APP_DIR" && npx jest --passWithNoTests --silent 2>&1 | tail -3 || true)
TEST_SUMMARY=$(echo "$TEST_OUTPUT" | grep -E "Tests:|Test Suites:" || echo "Tests: unknown")
if echo "$TEST_OUTPUT" | grep -q "failed"; then
  STATUS="red"
  ISSUES+="**Tests:** $TEST_SUMMARY\n"
fi

# --- Rust services test suite (#2117) ---
# Runs cargo test for each Rust service under platform/services/.
# On failure, collects failing test names, flags STATUS red, and nudges Silas
# separately from Kade (ops nudges Silas, quality nudges Kade — DEC-2243).
RUST_FAIL_DETAIL=""
for RUST_SVC in chorus-hooks chorus-inject; do
  RUST_DIR="${CHORUS_ROOT}/platform/services/${RUST_SVC}"
  [ -f "${RUST_DIR}/Cargo.toml" ] || continue
  CARGO_OUT=$(cd "$RUST_DIR" && cargo test --release 2>&1 || true)
  # `|| true` at end of pipe so no-match (all passed) doesn't trip pipefail
  CARGO_FAILS=$(echo "$CARGO_OUT" | grep -E '^test [A-Za-z0-9_:]+ \.\.\. FAILED$' | awk '{print $2}' | sort -u | head -10 || true)
  if [ -n "$CARGO_FAILS" ]; then
    STATUS="red"
    FAIL_COUNT=$(echo "$CARGO_FAILS" | wc -l | tr -d ' ')
    ISSUES+="**Rust (${RUST_SVC}):** ${FAIL_COUNT} failing\n"
    RUST_FAIL_DETAIL+="${RUST_SVC}: $(echo "$CARGO_FAILS" | tr '\n' ' ')\n"
  fi
done

# --- Build summary ---
if [ "$STATUS" = "green" ]; then
  BODY="🟢 **Quality Review** — $TIMESTAMP\n\nSmoke: ${SMOKE_PASS} pass. Lint: ${LINT_WARNINGS:-?} warnings. $TEST_SUMMARY"
elif [ "$STATUS" = "yellow" ]; then
  BODY="🟡 **Quality Review** — $TIMESTAMP\n\n$ISSUES"
else
  BODY="🔴 **Quality Review** — $TIMESTAMP\n\n$ISSUES"
fi

# --- Emit completion event (no Bridge post — summary script handles that) ---
"$CHORUS_LOG" quality.review.completed silas status=$STATUS >/dev/null 2>&1 || true


# --- Nudge Kade on quality failures (frontend: smoke/lint/jest) ---
NUDGE="$SCRIPT_DIR/nudge"
if [ "$STATUS" = "red" ]; then
  "$NUDGE" kade "[quality] $TIMESTAMP — $( echo -e "$ISSUES" | head -3 )" --force >/dev/null 2>&1 || true
fi

# --- Nudge Silas on Rust test failures + emit spine event (#2117) ---
if [ -n "$RUST_FAIL_DETAIL" ]; then
  "$NUDGE" silas "[nightly-tests] $TIMESTAMP — $( echo -e "$RUST_FAIL_DETAIL" | head -5 )" --force >/dev/null 2>&1 || true
  "$CHORUS_LOG" test.nightly.failed silas detail="$(echo -e "$RUST_FAIL_DETAIL" | tr '\n' ';' | head -c 400)" >/dev/null 2>&1 || true
fi

echo -e "$BODY"
