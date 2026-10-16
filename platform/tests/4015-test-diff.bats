#!/usr/bin/env bats
# @test-type: unit — hermetic: stubs the SPARQL seam with fixture CSV. No store,
# @domain: tests — the product domain this suite guards (#4334)
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
  [[ "$output" == *"no verdict changed"* ]] || return 1
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
  [[ "$output" == *"CHANGED  a.bats|two  pass -> fail"* ]] || return 1
  [[ "$output" == *"1 test(s) differ"* ]] || return 1
}

@test "a fail that recovered is named too — that is what a flake looks like" {
  mkstub '?k,?res
a.bats|one,fail' '?k,?res
a.bats|one,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"CHANGED  a.bats|one  fail -> pass"* ]] || return 1
}

@test "a test present in only one run is NEW or GONE, never a silent verdict change" {
  mkstub '?k,?res
a.bats|one,pass' '?k,?res
a.bats|one,pass
b.bats|added,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"NEW      b.bats|added"* ]] || return 1
}

@test "negative proof: an unanswerable store exits 2, never a reassuring 'no change'" {
  # A diff that cannot reach the ledger and prints "no verdict changed" is the
  # same lie as a suite reporting 0 pass 0 fail. Absence must refuse.
  printf '#!/bin/bash\nprintf ""\n' > "$TMP/ask"; chmod +x "$TMP/ask"
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 2 ]
  [[ "$output" != *"no verdict changed"* ]] || return 1
}

@test "control: the refusal is reachable only on a bad answer, not always" {
  # Without this the script could exit 2 unconditionally and still pass above.
  mkstub '?k,?res
a.bats|one,pass' '?k,?res
a.bats|one,pass'
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 0 ]
}

@test "negative proof (#4536): a full-size run is compared, never read as 'no change'" {
  # A whole nightly (~10,600 cases) once overflowed ARG_MAX on the way to the
  # comparison; python never ran and the empty output said "no verdict changed".
  python3 -c '
print("?k,?res")
for i in range(60000): print("suite.bats|case number %06d with a long enough name,pass" % i)' > "$TMP/a.csv"
  sed 's/case number 000007 with a long enough name,pass/case number 000007 with a long enough name,fail/' "$TMP/a.csv" > "$TMP/b.csv"
  cat > "$TMP/ask" <<EOF
#!/bin/bash
case "\$1" in
  *RUN_A*) cat "$TMP/a.csv" ;;
  *RUN_B*) cat "$TMP/b.csv" ;;
esac
EOF
  chmod +x "$TMP/ask"
  run "$DIFF" RUN_A RUN_B
  [ "$status" -eq 1 ]
  [[ "$output" == *"CHANGED  suite.bats|case number 000007 with a long enough name  pass -> fail"* ]] || return 1
}

# #4540 — under dagu a nightly's trace is its run id, which names no time, so
# its verdicts are read from its PipelineRun's runTs. A nightly-<ms>-<pid>
# trace names its own start and needs no lookup.
mkstub_logging() {
  cat > "$TMP/ask" <<'EOF2'
#!/bin/bash
echo "$1" >> "$TMP_LOG"
case "$1" in
  *c:traceId*034gpYTGEnSPcL7jOsEBMT*) printf '%s\n' '?ts' '"2026-10-16T03:00:01"' ;;
  LOKI*) printf '%s\n' '?k,?res' 'a.bats|one,pass' ;;
esac
EOF2
  chmod +x "$TMP/ask"
  export TMP_LOG="$TMP/asked"
}

@test "#4540 a dagu run id is read with its run's start" {
  mkstub_logging
  run "$DIFF" 034gpYTGEnSPcL7jOsEBMT nightly-1792049381065-39455
  grep -q '^LOKI 034gpYTGEnSPcL7jOsEBMT 2026-10-16T03:00:01$' "$TMP/asked"
}

@test "#4540 negative proof: a nightly-<ms>-<pid> trace asks no start" {
  mkstub_logging
  run "$DIFF" nightly-1792049381065-39455 nightly-1792049381066-39456
  run grep -c 'c:traceId' "$TMP/asked"
  [ "$output" = "0" ]
  grep -q '^LOKI nightly-1792049381065-39455 *$' "$TMP/asked"
}
