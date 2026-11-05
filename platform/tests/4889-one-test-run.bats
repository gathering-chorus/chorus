#!/usr/bin/env bats
# @test-type: unit — a round's tests run once, read from the generated workflows
# @domain: pipelines
# @card: #4889 · owner: silas
# Jeff 2026-11-04 18:21: "one test run". A round goes through pair (or mob), card,
# cicd (with its demo), then the go and the land. Only cicd's test step runs the tests;
# the pair's local proof is gone (#4752) and nothing else calls cicd-test or werk-test.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
FLOWS="pair mob card cicd demo cicd-go cicd-land"

# the steps in DIR's workflows that run the tests, one "<file>: <line>" each
test_runs() {
  local d="$1" f
  for f in $FLOWS; do
    grep -nE 'dag: cicd-test|command: .*(werk-test |cicd-test)' "$d/$f.yaml" | sed "s|^|$f: |"
  done
}

@test "AC3: one test run per round — cicd's test step, and only it" {
  out="$(test_runs "$ROOT/platform/pipelines")"
  [ "$(printf '%s\n' "$out" | grep -c .)" -eq 1 ] || { echo "$out"; return 1; }
  [[ "$out" == "cicd: "*"dag: cicd-test"* ]] || { echo "$out"; return 1; }
}

@test "negative proof: a pair that runs the tests again is a second test run" {
  d="$BATS_TEST_TMPDIR/p"; mkdir -p "$d"
  for f in $FLOWS; do cp "$ROOT/platform/pipelines/$f.yaml" "$d/"; done
  printf '  - name: skill-pair-local-proof\n    call:\n      dag: cicd-test\n' >> "$d/pair.yaml"
  [ "$(test_runs "$d" | grep -c .)" -eq 2 ] || { test_runs "$d"; return 1; }
}
