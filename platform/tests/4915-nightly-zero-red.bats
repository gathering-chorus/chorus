#!/usr/bin/env bats
# @test-type: integration — reads the newest 03:00 nightly's own suite lines; runs nothing
# @domain: tests
# @card: #4915 · owner: kade
# @needs: machine — reads prod after the land (AC check: after); a card run leaves it to borg-prove and the nightly (#4539)
# AC4 (checked after the land): the nightly reports 0 red suites. Jeff 11-06 06:13:
# "Should we mob all of our test errors and get things clean?" The verdict is the
# nightly's own `nightly-unit|<kind>|<suite>|<status>|<summary>` lines in its run log.
# An unmeasured suite is not green either (#4562): it counts against zero.

# the repo owner's home (two levels above the checkout), not a role account's $HOME
OWNER_HOME="$(cd "$BATS_TEST_DIRNAME/../../../../.." && pwd)"
RUNS="${NIGHTLY_RUNS_4915:-$OWNER_HOME/.chorus/nightly-failures/runs}"

verdict() {  # <lane-output.log> → 0 clean, 1 red/unmeasured listed, 2 no suite lines
  awk -F'|' '$1=="nightly-unit" { n++; if ($4!="pass" && $4!="skip") { bad++; print $4": "$3 } }
             END { if (!n) { print "UNMEASURED: no nightly-unit lines"; exit 2 } exit (bad ? 1 : 0) }' "$1"
}

@test "the newest nightly reports zero red and zero unmeasured suites" {
  local newest
  newest="$(ls -1dt "$RUNS"/nr-* 2>/dev/null | head -1)"
  [ -n "$newest" ] && [ -r "$newest/_lane-output.log" ] || { echo "no readable nightly run under $RUNS"; return 1; }
  echo "run: $newest"
  run verdict "$newest/_lane-output.log"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "negative proof: a run with one red suite, or one unmeasured, is not zero" {
  printf '%s\n' 'nightly-unit|bats|platform/tests/a.bats|pass|3 pass, 0 fail' \
                'nightly-unit|bats|platform/tests/b.bats|fail|1 pass, 1 fail' > "$BATS_TEST_TMPDIR/red.log"
  run verdict "$BATS_TEST_TMPDIR/red.log"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"fail: platform/tests/b.bats"* ]] || return 1
  printf '%s\n' 'nightly-unit|ui|proving/flows|unmeasured|0 pass, 0 fail' > "$BATS_TEST_TMPDIR/um.log"
  run verdict "$BATS_TEST_TMPDIR/um.log"
  [ "$status" -eq 1 ] || return 1
}

@test "negative proof: a run that reported no suites is never green" {
  printf 'nothing ran\n' > "$BATS_TEST_TMPDIR/empty.log"
  run verdict "$BATS_TEST_TMPDIR/empty.log"
  [ "$status" -eq 2 ] || return 1
}
