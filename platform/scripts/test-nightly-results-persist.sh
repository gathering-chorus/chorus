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
DISPATCH=$(sed -n '/--run-all)/,/;;/p' "$NIGHTLY")
if printf '%s' "$DISPATCH" | grep -qE '^\s*printf .*"\$out"' ; then
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
