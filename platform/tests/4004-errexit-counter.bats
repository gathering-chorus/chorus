#!/usr/bin/env bats
# @test-type: unit — pure bash semantics, no services, no fs beyond BATS_TEST_TMPDIR
#
# #4004 — 28 of 34 nightly reds were ONE bash gotcha, not 28 problems.
# `((PASS++))` evaluates to the value BEFORE the increment, so the first time a
# suite records a pass (PASS=0) the arithmetic returns 0 → exit status 1 → under
# the bats wrapper's errexit the whole suite dies. The log then blames the line
# that had just printed "PASS", which reads as an assertion failing while its
# own output says it passed. Every affected suite passes standalone, which is
# why this survived: it only appears once the suite runs under errexit.

@test "NEGATIVE PROOF: the old idiom KILLS a suite on its first recorded pass" {
  run bash -e -c 'record(){ echo "PASS: x"; ((PASS++)); }; PASS=0; record; echo REACHED_END'
  [ "$status" -eq 1 ]
  [[ "$output" != *"REACHED_END"* ]]
}

@test "the replacement survives errexit and still counts" {
  run bash -e -c 'record(){ echo "PASS: x"; PASS=$((PASS+1)); }; PASS=0; record; echo "REACHED_END n=$PASS"'
  [ "$status" -eq 0 ]
  [[ "$output" == *"REACHED_END n=1"* ]]
}

@test "no shell suite still uses the fatal idiom" {
  cd "$BATS_TEST_DIRNAME/.."
  bad=$(grep -rlE '\(\([A-Z_]+\+\+\)\)' scripts/ 2>/dev/null || true)
  if [ -n "$bad" ]; then
    echo "Still using ((VAR++)) — fatal under errexit when VAR is 0:"
    echo "$bad" | sed 's/^/  - /'
    false
  fi
}
