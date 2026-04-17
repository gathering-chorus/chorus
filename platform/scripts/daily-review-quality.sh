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

# --- All test suites (#2142) ---
# Every npm/cargo/shell suite discovered by nightly-suites.sh runs here.
# Per-suite lines: SUITE|<kind>|<path>|<owner>|<status>|<summary>
SUITES_OUT=$(bash "${SCRIPT_DIR}/nightly-suites.sh" --run-all 2>&1 || true)
TEST_SUMMARY=""
KADE_FAILS=""
SILAS_FAILS=""
SUITE_PASS=0
SUITE_FAIL=0
while IFS='|' read -r marker kind path owner status summary; do
  [ "$marker" = "SUITE" ] || continue
  name=$(basename "$path")
  case "$status" in
    pass) SUITE_PASS=$((SUITE_PASS+1)) ;;
    fail)
      SUITE_FAIL=$((SUITE_FAIL+1))
      line="${kind}:${name}: ${summary}"
      if [ "$owner" = "kade" ]; then
        KADE_FAILS+="${line}\n"
      else
        SILAS_FAILS+="${line}\n"
      fi
      ;;
  esac
done <<< "$SUITES_OUT"
TEST_SUMMARY="${SUITE_PASS} suites pass, ${SUITE_FAIL} fail"
if [ "$SUITE_FAIL" -gt 0 ]; then
  STATUS="red"
  ISSUES+="**Tests:** $TEST_SUMMARY\n"
fi

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


# --- Owner-routed nudges (#2142) ---
# Smoke + lint go to Kade (frontend quality). Per-suite fails route by owner.
NUDGE="$SCRIPT_DIR/nudge"
if [ "$STATUS" = "red" ] && [ "$SMOKE_FAIL" -gt 0 ] 2>/dev/null; then
  "$NUDGE" kade "[quality] $TIMESTAMP — smoke: ${SMOKE_FAIL} fail" --force >/dev/null 2>&1 || true
fi
if [ -n "${KADE_FAILS:-}" ]; then
  "$NUDGE" kade "[nightly-tests] $TIMESTAMP — $( echo -e "$KADE_FAILS" | head -5 )" --force >/dev/null 2>&1 || true
  "$CHORUS_LOG" test.nightly.failed kade detail="$(echo -e "$KADE_FAILS" | tr '\n' ';' | head -c 400)" >/dev/null 2>&1 || true
fi
if [ -n "${SILAS_FAILS:-}" ]; then
  "$NUDGE" silas "[nightly-tests] $TIMESTAMP — $( echo -e "$SILAS_FAILS" | head -5 )" --force >/dev/null 2>&1 || true
  "$CHORUS_LOG" test.nightly.failed silas detail="$(echo -e "$SILAS_FAILS" | tr '\n' ';' | head -c 400)" >/dev/null 2>&1 || true
fi

echo -e "$BODY"
