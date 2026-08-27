#!/usr/bin/env bats
# @test-type: unit — hermetic: stubs the SPARQL seam with fixture CSV. No store,
# no network.
#
# #4015 — nothing in this repo has ever compared two runs. `chorus:runTs` was
# added by #3925 precisely to make runs groupable, and every occurrence of it is
# the writer, the backfill, or a unit test — no consumer. So a run that goes
# 8 fail -> 5 fail can only be described as a number moving, and the difference
# gets called a flake because nobody can name the test.
#
# The three states that must stay separable, because "no change" and "could not
# ask" reading the same is the defect this card exists to remove:
#   no verdict changed   exit 0
#   something changed    exit 1, named
#   store did not answer exit 2, NEVER a reassuring empty result

DIFF="$BATS_TEST_DIRNAME/../scripts/test-diff"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  export TEST_DIFF_STUB="$TMP/ask"
}

# Stub the seam: return run-A rows for a query naming RUN_A, run-B rows for RUN_B.
mkstub() {
  cat > "$TMP/ask" <<EOF
#!/bin/bash
q="\$1"
case "\$q" in
  *RUN_A*) printf '%s\n' '$1' ;;
  *RUN_B*) printf '%s\n' '$2' ;;
  *)       printf '%s\n' '${3:-?t
"RUN_B"
"RUN_A"}' ;;
esac
EOF
  chmod +x "$TMP/ask"
}

@test "control: identical runs report no change and exit 0" {
  mkstub '?k,?res
a.bats|one,pass
a.bats|two,pass' '?k,?res
a.bats|one,pass
a.bats|two,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 0 ]
  [[ "$output" == *"no verdict changed"* ]]
}

@test "negative proof: a test that flipped pass->fail is NAMED, not counted" {
  # The whole point. Before this, the only available statement was "one more
  # failure than last night".
  mkstub '?k,?res
a.bats|one,pass
a.bats|two,pass' '?k,?res
a.bats|one,pass
a.bats|two,fail'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"CHANGED  a.bats|two  pass -> fail"* ]]
  [[ "$output" == *"1 test(s) differ"* ]]
}

@test "a fail that recovered is named too — that is what a flake looks like" {
  mkstub '?k,?res
a.bats|one,fail' '?k,?res
a.bats|one,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"CHANGED  a.bats|one  fail -> pass"* ]]
}

@test "a test present in only one run is NEW or GONE, never a silent verdict change" {
  mkstub '?k,?res
a.bats|one,pass' '?k,?res
a.bats|one,pass
b.bats|added,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"NEW      b.bats|added"* ]]
}

@test "negative proof: an unanswerable store exits 2, never a reassuring 'no change'" {
  # A diff that cannot reach the ledger and prints "no verdict changed" is the
  # same lie as a suite reporting 0 pass 0 fail. Absence must refuse.
  printf '#!/bin/bash\nprintf ""\n' > "$TMP/ask"; chmod +x "$TMP/ask"
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 2 ]
  [[ "$output" != *"no verdict changed"* ]]
}

@test "control: the refusal is reachable only on a bad answer, not always" {
  # Without this the script could exit 2 unconditionally and still pass above.
  mkstub '?k,?res
a.bats|one,pass' '?k,?res
a.bats|one,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 0 ]
}
