#!/usr/bin/env bash
# THE SECURITY FITNESS FUNCTION — run every row, emit every verdict. (#3765)
#
# Rows that only run when someone runs them depend on exactly the attention they
# exist to replace. This is the part that turns seven scripts into a measure.
#
# THE THING THIS DOES THAT NOTHING ELSE WE OWN DOES
#
# Every row is scored TWICE: once against reality, and once against the state it
# exists to catch. If a row's negative proof stops going red, the row is reported
# UNTRUSTWORTHY — never green, whatever the live run said.
#
# That is the only defence against the failure we hit four times in one day: a
# check that cannot distinguish the two states it exists to separate, sitting
# green. In every one of those four, the live run passed. What differed was
# whether the proof still failed for the reason it names. A suite that scores
# only the live run would have reported all four as healthy.
#
# VERDICTS
#   green         reality is as it should be, and the row can still see otherwise
#   red           reality is not as it should be
#   unmeasurable  the row could not ask — distinct from red, and never a pass
#   untrustworthy the row's negative proof no longer goes red; its green is void
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROLE="${DEPLOY_ROLE:-silas}"
ONLY="${1:-}"

emit() {  # emit <verdict> <row> <detail>
  local ev="fitness.row.$1"
  if command -v chorus-log >/dev/null 2>&1; then
    chorus-log "$ev" "$ROLE" "domain=chorus" "card=3765" \
      "row=$2" "detail=$3" >/dev/null 2>&1 || true
  fi
}

# row | script | negative-proof invocation
ROWS=(
  "1|probe-public-root.sh|--fixture $HERE/fixtures/login-wall-otherwise-perfect.html --expect-red"
  "2|probe-signin-lands.sh|--prove-red"
  "4|probe-running-is-merged.sh|--prove-red"
  "5|probe-write-still-works.sh|--prove-red"
  "6|probe-lists-are-local.sh|--prove-red"
  "7|probe-nobody-walk.sh|--prove-red"
  "8|probe-allowset-is-projection.sh|--prove-red"
)

# #3806 — every row runs under a hard ceiling. Row 7 sat 600s+ with no verdict
# on 2026-08-09: an unbounded probe in a nightly is one slow tunnel away from
# the whole readout reporting nothing. Expiry is UNMEASURABLE by definition —
# the row could not ask in the time it was given — never red, never a hang.
# (The snapshot layer carries its own ceiling too, deliberately: this one fixes
# the row, that one contains a row that fails to honor this.)
ROW_CEILING="${FITNESS_ROW_CEILING:-300}"
# run_bounded <script> [args...] — rc 142 on expiry.
# Two traps this shape exists to dodge, both hit while building it:
#   - a plain `perl alarm; exec` kills the SHELL but not its children, so a
#     probe's hung curl survives the ceiling;
#   - capturing via a pipe blocks until every fd holder exits, so the orphan
#     holds the readout hostage anyway. Output goes to a FILE; the child runs
#     in its own process GROUP and the whole group is killed on expiry.
run_bounded() {
  local out rc; out=$(mktemp)
  perl -e '
    my $t = shift @ARGV;
    my $pid = fork();
    if ($pid == 0) { setpgrp(0, 0); exec @ARGV or exit 127; }
    $SIG{ALRM} = sub { kill "KILL", -$pid };
    alarm $t;
    waitpid($pid, 0);
    exit((($? & 127) == 9) ? 142 : ($? >> 8));
  ' "$ROW_CEILING" "$@" >"$out" 2>&1
  rc=$?
  cat "$out"; rm -f "$out"
  return "$rc"
}

green=0; red=0; unmeasurable=0; untrustworthy=0
echo "SECURITY FITNESS — $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston"
echo

for entry in "${ROWS[@]}"; do
  IFS='|' read -r n script proof <<<"$entry"
  [ -n "$ONLY" ] && [ "$ONLY" != "$n" ] && continue
  s="$HERE/$script"
  if [ ! -x "$s" ]; then
    # A row whose script vanished must not be silently absent from the tally.
    echo "row$n  UNTRUSTWORTHY — $script is missing or not executable"
    untrustworthy=$((untrustworthy + 1)); emit untrustworthy "$n" "script missing"
    continue
  fi

  live=$(run_bounded "$s"); live_rc=$?
  if [ "$live_rc" -eq 142 ]; then
    echo "row$n  UNMEASURABLE — exceeded the ${ROW_CEILING}s ceiling; the row could not ask in the time it was given"
    unmeasurable=$((unmeasurable + 1)); emit unmeasurable "$n" "ceiling-${ROW_CEILING}s"
    continue
  fi

  # The proof runs on the SAME pass, against the SAME code. Running it only at
  # authoring time is how a check quietly stops being able to fail.
  # A proof that cannot finish inside the ceiling cannot vouch for anything —
  # that is untrustworthy, not unmeasurable: the row's green would be void.
  # shellcheck disable=SC2086
  proof_out=$(run_bounded "$s" $proof); proof_rc=$?
  if [ "$proof_rc" -eq 142 ]; then
    echo "row$n  UNTRUSTWORTHY — its negative proof exceeded the ${ROW_CEILING}s ceiling; any green it reports is void"
    untrustworthy=$((untrustworthy + 1)); emit untrustworthy "$n" "proof-ceiling-${ROW_CEILING}s"
    continue
  fi

  if [ "$proof_rc" -ne 0 ]; then
    echo "row$n  UNTRUSTWORTHY — its negative proof no longer goes red; any green it reports is void"
    echo "        $(head -1 <<<"$proof_out")"
    untrustworthy=$((untrustworthy + 1)); emit untrustworthy "$n" "negative proof did not fail"
    continue
  fi

  case "$live_rc" in
    0) echo "row$n  green"; green=$((green + 1)); emit green "$n" "ok" ;;
    2) echo "row$n  UNMEASURABLE — $(head -1 <<<"$live")"
       unmeasurable=$((unmeasurable + 1)); emit unmeasurable "$n" "could not ask" ;;
    *) echo "row$n  RED"; sed 's/^/        /' <<<"$live"
       red=$((red + 1)); emit red "$n" "$(head -c 300 <<<"$live" | tr '\n' ' ')" ;;
  esac
done

echo
echo "green=$green red=$red unmeasurable=$unmeasurable untrustworthy=$untrustworthy"
emit summary all "green=$green red=$red unmeasurable=$unmeasurable untrustworthy=$untrustworthy"

# Untrustworthy counts as failure. A row that cannot fail is worse than a row
# that is failing: the failing one is telling you something.
[ $((red + untrustworthy)) -eq 0 ]
