#!/usr/bin/env bats
# @test-type: unit — signal:api is fixture-data: the URLs are stub values passed to a stub athena-validate, nothing is called
# @domain: athena
# @card: #4467
# @owner: wren
#
# #4467 — the land's prove step checks the graphs the card's model feeds, held to
# the same graphs' count in prod before the land. The whole-store sweep (10.7M
# triples) timed three checks out and every land said UNMEASURED. The steps are
# pulled out of athena.yml and run with a stub athena-validate, the #4336 way.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
YML="$ROOT/.github/workflows/athena.yml"
FIX="$ROOT/platform/tests/fixtures/4336"

setup() {
  T="$(mktemp -d "$BATS_TEST_TMPDIR/t.XXXXXX")"
  mkdir -p "$T/home/platform/scripts" "$T/h/.chorus/bin"
  : > "$T/gh.env"; : > "$T/gh.out"
  : > "$T/home/platform/scripts/fuseki-auth.sh"
  # the stub reports STUB_N issues (or no count when unset) and records its scope
  printf '%s\n' '#!/bin/bash' \
    'echo "$ATHENA_VALIDATE_GRAPHS|$FUSEKI_QUERY|$CHORUS_OWL_API" >> "$STUB_SEEN"' \
    'echo "graph-scope|$ATHENA_VALIDATE_GRAPHS|SCOPED"' \
    'if [ -n "${STUB_N:-}" ]; then echo "graph-issue|x|y|z"; echo "graph-summary|$STUB_N|dirty"; exit 1; fi' \
    'echo "graph-summary|UNMEASURED|unreachable"; exit 2' > "$T/h/.chorus/bin/athena-validate"
  chmod +x "$T/h/.chorus/bin/athena-validate"
}

run_step() {
  local name="$1"; shift
  ruby "$FIX/gha-step.rb" "$YML" land "$name" "$@" > "$T/step.sh" || return 1
  local envs=()
  while IFS= read -r l; do [ -n "$l" ] && envs+=("$l"); done < "$T/gh.env"
  run env -i PATH="/usr/bin:/bin" HOME="$T/h" CHORUS_HOME="$T/home" \
    GITHUB_ENV="$T/gh.env" GITHUB_OUTPUT="$T/gh.out" STUB_SEEN="$T/seen" \
    CARD_ID=4467 ROLE=wren TARGET=werk LANDED= \
    "${envs[@]}" ${STEP_ENV[@]+"${STEP_ENV[@]}"} bash -e "$T/step.sh"
}

ARGS=("steps.scope.outputs.graphs=urn:chorus:domains:skills" "steps.resolve.outputs.store=http://127.0.0.1:9/staging" \
      "steps.resolve.outputs.api=http://127.0.0.1:9" "steps.scope.outputs.model=roles/wren/ontology/skills-4467.ttl")

@test "prove-before measures prod's copy of the card's graphs and records the count" {
  STEP_ENV=(STUB_N=4)
  run_step prove-before "${ARGS[@]}"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  grep -qx 'ATHENA_ISSUES_BEFORE=4' "$T/gh.env"
  grep -q '^urn:chorus:domains:skills|http://localhost:3030/pods/query|http://localhost:3360$' "$T/seen"
}

@test "prove passes when the card's graphs have no more gaps than before, scoped to those graphs" {
  echo "ATHENA_ISSUES_BEFORE=4" > "$T/gh.env"
  STEP_ENV=(STUB_N=4)
  run_step prove "${ARGS[@]}"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  [[ "$output" == *"4 issues in this card's graphs (4 before"* ]] || false
  grep -q '^urn:chorus:domains:skills|http://127.0.0.1:9/staging/query|http://127.0.0.1:9$' "$T/seen"
}

@test "NEGATIVE PROOF — prove fails when the card added gaps, and lists them" {
  echo "ATHENA_ISSUES_BEFORE=4" > "$T/gh.env"
  STEP_ENV=(STUB_N=5)
  run_step prove "${ARGS[@]}"
  [ "$status" -ne 0 ] || false
  [[ "$output" == *"ADDED gaps"* ]] || false
  [[ "$output" == *"graph-issue|x|y|z"* ]] || false
}

@test "NEGATIVE PROOF — an unmeasured sweep is never proven, and says what was unmeasured" {
  echo "ATHENA_ISSUES_BEFORE=4" > "$T/gh.env"
  run_step prove "${ARGS[@]}"
  [ "$status" -ne 0 ] || false
  [[ "$output" == *"graph-summary|UNMEASURED"* ]] || false
}

@test "NEGATIVE PROOF — prove with no before-count refuses rather than pass" {
  STEP_ENV=(STUB_N=0)
  run_step prove "${ARGS[@]}"
  [ "$status" -ne 0 ] || false
  [[ "$output" == *"prove-before left no count"* ]] || false
}
