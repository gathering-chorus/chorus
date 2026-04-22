#!/bin/bash
# daily-review-quality.sh — 6am quality check, posts to Bridge
# Card #1766 | DEC-107 compliant (no osascript)
set -euo pipefail

# Hermeticity gate (#2131, #2149): nightly runs must not fire real nudges,
# bridge writes, or terminal injections. Test files honoring this env var
# skip their real-I/O describes. Interactive runs (/demo smoke) unset it.
export HERMETIC_TEST_MODE=1

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

# --- All test suites (#2142, #2438) ---
# Every npm/cargo/shell suite discovered by nightly-suites.sh runs here.
# Per-suite lines: SUITE|<kind>|<path>|<owner>|<status>|<summary>
#
# #2438: Report test-level counts, not just suite pass/fail. A single failing
# test in a 1000-test suite should not read the same as a 1000-test suite that
# could not compile. Three categories now: all-green, with ≥1 failing test,
# did not run.
SUITES_OUT=$(bash "${SCRIPT_DIR}/nightly-suites.sh" --run-all 2>&1 || true)
TEST_SUMMARY=""
KADE_FAILS=""
SILAS_FAILS=""
SUITE_ALL_GREEN=0
SUITE_WITH_FAIL=0
SUITE_NO_RUN=0
TOTAL_TESTS=0
FAILED_TESTS=0
while IFS='|' read -r marker kind path owner status summary; do
  [ "$marker" = "SUITE" ] || continue
  name=$(basename "$path")

  s_total=0; s_failed=0; s_passed=0; parsed="no"
  case "$kind" in
    npm)
      if echo "$summary" | grep -qE "[0-9]+ total"; then
        s_total=$(echo "$summary" | grep -oE '[0-9]+ total' | head -1 | grep -oE '[0-9]+')
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+' || echo 0)
        parsed="yes"
      fi
      ;;
    cargo)
      # cargo summary counts sub-suites, not individual tests; each sub-suite
      # counted as one unit for accounting.
      if echo "$summary" | grep -qE "suites:"; then
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ ok' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_total=$((s_passed + s_failed))
        [ "$s_total" -gt 0 ] && parsed="yes"
      fi
      ;;
    shell)
      if echo "$summary" | grep -qE "[0-9]+ (pass|ok)"; then
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ (pass|ok)' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ fail' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_total=$((s_passed + s_failed))
        [ "$s_total" -gt 0 ] && parsed="yes"
      fi
      ;;
  esac
  : "${s_total:=0}" "${s_failed:=0}" "${s_passed:=0}"

  if [ "$parsed" = "no" ]; then
    SUITE_NO_RUN=$((SUITE_NO_RUN+1))
    line="${kind}:${name}: DID NOT RUN (no parseable test output)"
    if [ "$owner" = "kade" ]; then KADE_FAILS+="${line}\n"; else SILAS_FAILS+="${line}\n"; fi
  elif [ "$s_failed" -gt 0 ]; then
    SUITE_WITH_FAIL=$((SUITE_WITH_FAIL+1))
    TOTAL_TESTS=$((TOTAL_TESTS + s_total))
    FAILED_TESTS=$((FAILED_TESTS + s_failed))
    line="${kind}:${name}: ${s_failed}/${s_total} failed"
    if [ "$owner" = "kade" ]; then KADE_FAILS+="${line}\n"; else SILAS_FAILS+="${line}\n"; fi
  else
    SUITE_ALL_GREEN=$((SUITE_ALL_GREEN+1))
    TOTAL_TESTS=$((TOTAL_TESTS + s_total))
  fi
done <<< "$SUITES_OUT"

SUITE_TOTAL=$((SUITE_ALL_GREEN + SUITE_WITH_FAIL + SUITE_NO_RUN))
PASSED_TESTS=$((TOTAL_TESTS - FAILED_TESTS))
if [ "$TOTAL_TESTS" -gt 0 ]; then
  PASS_PCT=$(awk -v p=$PASSED_TESTS -v t=$TOTAL_TESTS 'BEGIN { printf "%.2f", (p / t) * 100 }')
else
  PASS_PCT="n/a"
fi

TEST_SUMMARY="${SUITE_TOTAL} suites: ${SUITE_ALL_GREEN} all-green, ${SUITE_WITH_FAIL} with ≥1 failing test, ${SUITE_NO_RUN} did not run; ${FAILED_TESTS} failing tests / ${TOTAL_TESTS} total (${PASS_PCT}% pass)"

# #2438 wave 2 (Wren): surface failing-suite names inline so triage doesn't
# need a separate fetch. Top 5 with ellipsis; combined across kade+silas.
ALL_FAILS=$(echo -e "${KADE_FAILS}${SILAS_FAILS}" | grep -v '^$' | sed -E 's|^[^:]+:([^:]+):.*|\1|' | sort -u)
FAIL_COUNT=$(echo "$ALL_FAILS" | grep -c . 2>/dev/null || echo 0)
if [ "$FAIL_COUNT" -gt 0 ]; then
  FAIL_HEAD=$(echo "$ALL_FAILS" | head -5 | tr '\n' ',' | sed 's/,$//' | sed 's/,/, /g')
  if [ "$FAIL_COUNT" -gt 5 ]; then
    FAIL_NAMES_LINE="failing: ${FAIL_HEAD}, …"
  else
    FAIL_NAMES_LINE="failing: ${FAIL_HEAD}"
  fi
  TEST_SUMMARY="${TEST_SUMMARY}\n${FAIL_NAMES_LINE}"
fi

# Legacy names kept so the chorus-log payload + nudge routing below stay intact.
SUITE_FAIL=$((SUITE_WITH_FAIL + SUITE_NO_RUN))
SUITE_PASS=$SUITE_ALL_GREEN

# Status escalation:
#   red    — any suite could not run, OR pass-rate ≤ 99%
#   yellow — tests ran with failures but pass-rate > 99% (decay, not catastrophe)
if [ "$SUITE_NO_RUN" -gt 0 ]; then
  STATUS="red"
  ISSUES+="**Tests:** $TEST_SUMMARY\n"
elif [ "$SUITE_WITH_FAIL" -gt 0 ]; then
  HIGH_PASS=$(awk -v p="$PASS_PCT" 'BEGIN { print (p > 99.0) ? 1 : 0 }')
  if [ "$HIGH_PASS" = "1" ]; then
    [ "$STATUS" = "green" ] && STATUS="yellow"
  else
    STATUS="red"
  fi
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
