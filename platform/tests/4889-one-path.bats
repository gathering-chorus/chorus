#!/usr/bin/env bats
# @test-type: unit — the proof steps a card passes through, read from the generated workflows
# @domain: pipelines
# @card: #4889 · owner: silas
# Jeff 2026-11-04 18:21: "one test run · one code review · one architecture and
# quality review · prove in prod", plus his go. Nothing else between push and land.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# one line per proof step present in the workflows under DIR
steps() {
  local p="$1"
  grep -q 'pipeline-start handoff' "$p/pair.yaml" && grep -q 'local.proof\|local_proof\|cicd-test' "$p/pair.yaml" && echo "pair test run"
  grep -q 'name: skill-werk-test-pipeline' "$p/cicd.yaml" && echo "test run"
  grep -q 'name: skill-werk-review$' "$p/cicd.yaml" && echo "review floor"
  for g in product ops; do grep -q "name: skill-werk-demo-gate-$g$" "$p/demo.yaml" && echo "gate $g"; done
  grep -q 'name: skill-werk-demo-review-' "$p/demo.yaml" && echo "peer reviews"
  # each review step calls the skill that already exists (Jeff 18:22: no new names)
  grep -q 'command: "werk-demo run-gate ${CARD} ${ROLE} code"' "$p/demo.yaml" && echo "code review"
  grep -q 'command: "werk-demo run-gate ${CARD} ${ROLE} arch"' "$p/demo.yaml" \
    && grep -q 'command: "werk-demo run-gate ${CARD} ${ROLE} quality"' "$p/demo.yaml" && echo "architecture and quality review"
  grep -q 'name: skill-werk-demo-session-smoke' "$p/demo.yaml" && echo "demo smoke"
  grep -q 'name: skill-werk-go' "$p/cicd-go.yaml" && echo "jeff go"
  grep -q 'name: skill-borg-verify-in-prod' "$p/cicd-land.yaml" && echo "prove in prod"
  return 0
}
WANT="test run
code review
architecture and quality review
jeff go
prove in prod"

@test "AC2: a card's proof path is exactly Jeff's: one test run, one code review, one architecture and quality review, prove in prod, his go" {
  got="$(steps "$ROOT/platform/pipelines")"
  [ "$got" = "$WANT" ] || { echo "got:"; echo "$got"; return 1; }
}

@test "AC2 negative proof: today's main, with the old layers, is not Jeff's path" {
  d="$BATS_TEST_TMPDIR/p"; mkdir -p "$d"
  for f in pair cicd demo cicd-go cicd-land; do git -C "$ROOT" show "origin/main:platform/pipelines/$f.yaml" > "$d/$f.yaml"; done
  got="$(steps "$d")"
  [ "$got" != "$WANT" ] || return 1
  [[ "$got" == *"gate product"* && "$got" == *"demo smoke"* ]] || { echo "$got"; return 1; }
}
