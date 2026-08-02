#!/usr/bin/env bash
# #3709 — the nightly must PERSIST its own results.
#
# The failure this pins: nightly-suites.sh --run-all printed its SUITE| lines to
# stdout and nothing else. The aggregate log was produced solely by launchd's
# StandardOutPath redirect. When that redirect stopped landing (2026-07-22), the
# suites kept running and kept writing per-suite failure files, but the results
# reached nobody — daily-review-quality.sh reads only the aggregate. Seven days
# of red went unreported and --last-run correctly said "the 03:00 run did not
# write" while everyone read it as "the nightly did not run".
#
# A results file that exists only if an external redirect happens to work is a
# single point of failure for the entire reporting chain. The script owns it now.
#
# Hermetic: sources the script (source-guard at the bottom), calls the persist
# function directly with a fixture. No suite executes.
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok(){ PASS=$((PASS+1)); echo "ok   - $1"; }
no(){ FAIL=$((FAIL+1)); echo "FAIL - $1"; }

# shellcheck disable=SC1090
source "$NIGHTLY"
if ! command -v persist_run_results >/dev/null; then
  echo "FAIL - nightly-suites.sh does not expose persist_run_results (results still depend on the launchd redirect)"
  exit 1
fi

FIXTURE='SUITE|shell|/x/a.sh|silas|pass|1 passed, 0 failed
SUITE|shell|/x/b.sh|silas|fail|0 passed, 2 failed'

# 1. The redirect is BROKEN (stdout goes to /dev/null, as it effectively had
#    been since 07-22). The results must still land in the log.
LOG="$TMP/broken-redirect.log"
NIGHTLY_LOG_PATH="$LOG" persist_run_results "$FIXTURE" >/dev/null 2>&1
if [ -s "$LOG" ] && [ "$(grep -c '^SUITE|' "$LOG")" -eq 2 ]; then
  ok "results land in the log when the stdout redirect is broken"
else
  no "results LOST when the redirect is broken — got $(grep -c '^SUITE|' "$LOG" 2>/dev/null || echo 0) of 2 lines"
fi

# 2. Appends, never truncates: --last-run walks backward through concatenated
#    runs to find the block boundary, so a second run must not erase the first.
NIGHTLY_LOG_PATH="$LOG" persist_run_results "$FIXTURE" >/dev/null 2>&1
[ "$(grep -c '^SUITE|' "$LOG")" -eq 4 ] \
  && ok "appends a second run (4 lines) rather than truncating" \
  || no "second run did not append — got $(grep -c '^SUITE|' "$LOG") lines, want 4"

# 3. And --last-run can read back exactly the LAST run from what we wrote.
OUT=$(NIGHTLY_LOG_PATH="$LOG" bash "$NIGHTLY" --last-run 2>&1)
[ "$(printf '%s\n' "$OUT" | grep -c '^SUITE|')" -eq 2 ] \
  && ok "--last-run reads back exactly one run's worth of results" \
  || no "--last-run could not read what persist wrote: $(printf '%s' "$OUT" | head -2)"

# 4. Exactly one copy per run, even when the launchd redirect IS working.
#    persist_run_results is the SINGLE writer: --run-all only echoes to stdout
#    when fd 1 is a terminal, so under launchd (fd 1 = the log) nothing is
#    written twice. Assert the invariant at its source — the dispatch must not
#    unconditionally print the results it is also persisting.
#    #3721 — this pattern was `^\s*printf .*"\$out"`, and it went FALSE RED when
#    #3720 added the incremental-persistence markers. That commit introduced
#      printf 'RUN|complete|%s|suites=%s\n' ... "$(printf '%s\n' "$out" | grep -c ...)"
#    which begins with printf and contains "$out" inside a command substitution,
#    so `.*` happily spanned into the subshell and flagged it. The guarded echo
#    it actually exists to catch — `[ -t 1 ] && printf '%s\n' "$out"` — does not
#    start with printf and never matched either way. The invariant held the whole
#    time; only the pattern was wrong, and it failed the suite on correct code.
#    Now anchored to a BARE, whole-line print of the results: a tty-guarded line
#    cannot match (it starts with `[`), and a RUN| marker cannot match (its
#    format string is not '%s\n').
# #3725 — the extraction stopped at the FIRST `;;` after `--run-all)`. Silas's
# #3722 then added a NESTED `case "$CHORUS_ROOT" in ... fi ;;` inside that arm
# (werk log-isolation), so the range now truncated there and never reached
# `persist_run_results` 60 lines further down. Result: "--run-all does not call
# persist_run_results" — a FALSE RED on correct code, currently failing on
# canonical main too. Exactly the failure this file already suffered once: in
# #3721 its sibling check false-fired when #3720 added a printf the pattern
# didn't anticipate. A crude extractor breaks every time the code it reads over
# grows a new construct.
# Now terminate on a line that is ONLY whitespace + `;;` — the arm terminator at
# its own indent level. A nested `fi ;;` / `esac ;;` has content before the `;;`
# and no longer ends the range.
DISPATCH=$(awk '/--run-all\)/{f=1} f{print} f && /^[[:space:]]*;;[[:space:]]*$/{exit}' "$NIGHTLY")
if printf '%s' "$DISPATCH" | grep -qE "^[[:space:]]*printf[[:space:]]+'%s\\\\n'[[:space:]]+\"\\\$out\"[[:space:]]*$" ; then
  no "dispatch prints the results unconditionally AND persists them — duplicates under a working redirect"
else
  ok "dispatch does not unconditionally echo results it also persists"
fi
printf '%s' "$DISPATCH" | grep -q 'persist_run_results' \
  && ok "--run-all persists its results" \
  || no "--run-all does not call persist_run_results"

# 5. An unwritable log is LOUD. Silence here is what cost a week.
RO="$TMP/readonly"; mkdir -p "$RO"; chmod 500 "$RO"
UNWRITABLE="$RO/x.log"
ERR=$(NIGHTLY_LOG_PATH="$UNWRITABLE" persist_run_results "$FIXTURE" 2>&1 >/dev/null)
chmod 700 "$RO"   # so the EXIT trap can clean up
case "$ERR" in
  *"$UNWRITABLE"*|*WARN*|*warning*) ok "warns when it cannot persist results" ;;
  *) no "failed to persist SILENTLY — the exact failure mode this card exists for" ;;
esac

echo "---"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
