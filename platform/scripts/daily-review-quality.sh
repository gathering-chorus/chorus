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

# classify_suite <kind> <status> <summary> → echoes "verdict|passed|failed|total"
# #3571: EXIT CODE IS THE VERDICT. `status` is nightly-suites.sh's rc-derived
# verdict (pass|fail|skip; empty = the suite emitted no verdict at all), and it is
# AUTHORITATIVE. `summary` is parsed ONLY for the counts (enrichment). The old
# code ignored `status` and RE-DERIVED a verdict from artifact text — so a passing
# crate whose output the parser couldn't read read "DID NOT RUN", and an rc≠0 run
# read a false "1/1 failed". That re-derivation WAS the ~37-false-reds/night bug.
#   verdict ∈ green | fail | broke | skip | norun
# Extracted (#3537) so the nightly consumer AND test-daily-review-lint-parse.sh call
# the SAME logic — the test guards real code, not a mirror copy.
classify_suite() {
  local kind="$1" status="$2" summary="$3"
  local s_total=0 s_failed=0 s_passed=0

  # Verdict from status first. Empty status = no rc emitted = genuine did-not-run.
  case "$status" in
    skip) printf 'skip|0|0|0\n';  return ;;
    "")   printf 'norun|0|0|0\n'; return ;;
  esac

  # Parse counts (enrichment ONLY — never the verdict) — best-effort per kind.
  case "$kind" in
    npm)
      if echo "$summary" | grep -qE "[0-9]+ total"; then
        s_total=$(echo "$summary" | grep -oE '[0-9]+ total' | head -1 | grep -oE '[0-9]+')
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+' || echo 0)
      fi
      ;;
    cargo)
      # cargo summary counts sub-suites, not individual tests; each sub-suite = one unit.
      if echo "$summary" | grep -qE "suites:"; then
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ ok' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_total=$((s_passed + s_failed))
      fi
      ;;
    shell|lint|coverage)
      # #3600 — coverage emits the same "N pass, N fail (coverage % vs floor)" shape as
      # shell/lint. Without `coverage` here a coverage-floor MISS fell through to the broke
      # verdict → mislabeled "BUILD BROKE (rc≠0, no test output)" though coverage ran fine.
      if echo "$summary" | grep -qE "[0-9]+ (pass|ok)"; then
        s_passed=$(echo "$summary" | grep -oE '[0-9]+ (pass|ok)' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_failed=$(echo "$summary" | grep -oE '[0-9]+ fail' | head -1 | grep -oE '[0-9]+' || echo 0)
        s_total=$((s_passed + s_failed))
      fi
      ;;
  esac

  if [ "$status" = "fail" ]; then
    if [ "${s_failed:-0}" -gt 0 ]; then
      # rc≠0 with a parseable failing count → real test failure(s)
      [ "${s_total:-0}" -gt 0 ] || s_total=$s_failed   # real fallback (s_total is "0", not empty)
      printf 'fail|%s|%s|%s\n' "${s_passed:-0}" "$s_failed" "$s_total"
    else
      # rc≠0 but no test count (compile/build died before tests ran) → BUILD BROKE,
      # a distinct reader-signal from N-tests-failed (both RED, different reason).
      printf 'broke|%s|1|1\n' "${s_passed:-0}"
    fi
  else
    # status=pass → GREEN even if the summary didn't parse — THE false-red fix.
    printf 'green|%s|0|%s\n' "${s_passed:-0}" "${s_total:-${s_passed:-0}}"
  fi
}

# When SOURCED (by the parser-test), expose classify_suite and stop — don't run the 6am
# review (which shells the full nightly suite). When EXECUTED, fall through and run normally.
[ "${BASH_SOURCE[0]:-}" = "${0}" ] || return 0

# --- Smoke + app-eslint: MOVED into nightly-suites.sh (#3527 fold; standalone runs removed #3569) ---
# Both now run as SUITE tiers INSIDE nightly-suites.sh (executed below at --run-all) and are
# processed by the suite loop, so running them standalone here too was a DOUBLE-RUN. Removed.
# Their pass/fail now flows through the suite loop's status escalation + KADE_FAILS routing,
# same as every other suite. (SMOKE_PASS / LINT_WARNINGS no longer set here — BODY + nudges updated.)

# --- All test suites (#2142, #2438) ---
# Every npm/cargo/shell suite discovered by nightly-suites.sh runs here.
# Per-suite lines: SUITE|<kind>|<path>|<owner>|<status>|<summary>
#
# #2438: Report test-level counts, not just suite pass/fail. A single failing
# test in a 1000-test suite should not read the same as a 1000-test suite that
# could not compile. Three categories now: all-green, with ≥1 failing test,
# did not run.
# #3606/#3272 — READ the 03:00 nightly's results; never re-run. The 6 AM
# launchd env has no cargo on PATH, so a re-run manufactured a 30-suite
# false-red wall (2026-07-04) and clobbered the per-suite failure logs.
# --last-run fails loud (parseable SUITE|meta fail) on a missing/stale log.
SUITES_OUT=$(bash "${SCRIPT_DIR}/nightly-suites.sh" --last-run 2>&1 || true)
TEST_SUMMARY=""
KADE_FAILS=""
SILAS_FAILS=""
SUITE_ALL_GREEN=0
SUITE_WITH_FAIL=0
SUITE_NO_RUN=0
SUITE_SKIP=0
TOTAL_TESTS=0
FAILED_TESTS=0
while IFS='|' read -r marker kind path owner status summary; do
  [ "$marker" = "SUITE" ] || continue
  name=$(basename "$path")

  # #3571: verdict FROM status (exit-code-is-verdict); classify_suite parses counts only.
  IFS='|' read -r verdict s_passed s_failed s_total <<< "$(classify_suite "$kind" "$status" "$summary")"
  : "${s_total:=0}" "${s_failed:=0}" "${s_passed:=0}"

  case "$verdict" in
    skip)
      SUITE_SKIP=$((SUITE_SKIP+1))
      ;;
    norun)
      SUITE_NO_RUN=$((SUITE_NO_RUN+1))
      line="${kind}:${name}: DID NOT RUN (no verdict emitted)"
      if [ "$owner" = "kade" ]; then KADE_FAILS+="${line}\n"; else SILAS_FAILS+="${line}\n"; fi
      ;;
    broke)
      # #3571 (Silas nit): a build-broke has no tests — count it at the SUITE level
      # only (SUITE_WITH_FAIL), don't touch the test-level FAILED_TESTS tally (which
      # would conflate it with a failed test + push PASSED_TESTS negative if all broke).
      SUITE_WITH_FAIL=$((SUITE_WITH_FAIL+1))
      line="${kind}:${name}: BUILD BROKE (rc≠0, no test output)"
      if [ "$owner" = "kade" ]; then KADE_FAILS+="${line}\n"; else SILAS_FAILS+="${line}\n"; fi
      ;;
    fail)
      SUITE_WITH_FAIL=$((SUITE_WITH_FAIL+1))
      TOTAL_TESTS=$((TOTAL_TESTS + s_total))
      FAILED_TESTS=$((FAILED_TESTS + s_failed))
      line="${kind}:${name}: ${s_failed}/${s_total} failed"
      if [ "$owner" = "kade" ]; then KADE_FAILS+="${line}\n"; else SILAS_FAILS+="${line}\n"; fi
      ;;
    *)  # green
      SUITE_ALL_GREEN=$((SUITE_ALL_GREEN+1))
      TOTAL_TESTS=$((TOTAL_TESTS + s_total))
      ;;
  esac
done <<< "$SUITES_OUT"

SUITE_TOTAL=$((SUITE_ALL_GREEN + SUITE_WITH_FAIL + SUITE_NO_RUN + SUITE_SKIP))
PASSED_TESTS=$((TOTAL_TESTS - FAILED_TESTS))
if [ "$TOTAL_TESTS" -gt 0 ]; then
  PASS_PCT=$(awk -v p=$PASSED_TESTS -v t=$TOTAL_TESTS 'BEGIN { printf "%.2f", (p / t) * 100 }')
else
  PASS_PCT="n/a"
fi

TEST_SUMMARY="${SUITE_TOTAL} suites: ${SUITE_ALL_GREEN} all-green, ${SUITE_WITH_FAIL} with ≥1 failing/broke, ${SUITE_SKIP} skipped (no stack), ${SUITE_NO_RUN} did not run; ${FAILED_TESTS} failing tests / ${TOTAL_TESTS} total (${PASS_PCT}% pass)"

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
  BODY="🟢 **Quality Review** — $TIMESTAMP\n\n$TEST_SUMMARY"
elif [ "$STATUS" = "yellow" ]; then
  BODY="🟡 **Quality Review** — $TIMESTAMP\n\n$ISSUES"
else
  BODY="🔴 **Quality Review** — $TIMESTAMP\n\n$ISSUES"
fi

# --- Emit completion event (no Bridge post — summary script handles that) ---
"$CHORUS_LOG" quality.review.completed silas status=$STATUS >/dev/null 2>&1 || true


# --- Owner-routed nudges (#2142) ---
# Smoke + lint go to Kade (frontend quality). Per-suite fails route by owner.
OPS_NUDGE="$SCRIPT_DIR/ops-nudge"
# (smoke-specific nudge removed #3569 — smoke is now a SUITE tier, its fails route via KADE_FAILS below)
if [ -n "${KADE_FAILS:-}" ]; then
  "$OPS_NUDGE" kade "[nightly-tests] $TIMESTAMP — $( echo -e "$KADE_FAILS" | head -5 )" >/dev/null 2>&1 || true
  "$CHORUS_LOG" test.nightly.failed kade detail="$(echo -e "$KADE_FAILS" | tr '\n' ';' | head -c 400)" >/dev/null 2>&1 || true
fi
if [ -n "${SILAS_FAILS:-}" ]; then
  "$OPS_NUDGE" silas "[nightly-tests] $TIMESTAMP — $( echo -e "$SILAS_FAILS" | head -5 )" >/dev/null 2>&1 || true
  "$CHORUS_LOG" test.nightly.failed silas detail="$(echo -e "$SILAS_FAILS" | tr '\n' ';' | head -c 400)" >/dev/null 2>&1 || true
fi

echo -e "$BODY"
