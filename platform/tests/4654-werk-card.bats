#!/usr/bin/env bats
# @test-type: unit — the card pipeline's steps, with stubs for the runner, the starter and the nudge
# @domain: cicd
# What Jeff sees (#4654): a card's run starts at its pull. The role is told how
# to say its code is done; only that role's "done" moves the card on (Jeff may
# end it); and the hand-off starts athena for a model card, cicd for any other.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  WC="$ROOT/platform/scripts/werk-card"
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$T/home" CHORUS_WERK_BASE="$T/werks" HOME="$T"
  mkdir -p "$CHORUS_HOME/platform/scripts" "$CHORUS_WERK_BASE"
  printf '#!/bin/sh\nexit 0\n' > "$CHORUS_HOME/platform/scripts/chorus-log"
  # stubs record what they were asked to do
  printf '#!/bin/sh\necho "runner ROLE=$ROLE $*" >> "%s/calls"\n[ -n "$STUB_RUNNER_FAIL" ] && { echo refused; exit 1; }\nexit 0\n' "$T" > "$T/runner"
  printf '#!/bin/sh\necho "start $*" >> "%s/calls"\necho "enqueued $1"\n' "$T" > "$T/start"
  printf '#!/bin/sh\necho "nudge $*" >> "%s/calls"\n' "$T" > "$T/nudge"
  chmod +x "$T/runner" "$T/start" "$T/nudge"
  export WERK_CARD_RUNNER="$T/runner" WERK_CARD_START="$T/start" WERK_CARD_NUDGE="$T/nudge" DAG_RUN_ID=run-1
}

werk_with() { # a card's werk off a main, with the named file changed
  local w="$CHORUS_WERK_BASE/kade-$1"
  git init -q -b main "$T/origin" && git -C "$T/origin" commit -q --allow-empty -m base
  git clone -q "$T/origin" "$w"
  mkdir -p "$(dirname "$w/$2")" && echo x > "$w/$2"
}

@test "pull: werk-pull runs as the card's role, and the role is told the done command for this run" {
  run "$WC" pull 4654 kade
  [ "$status" -eq 0 ]
  grep -q "runner ROLE=kade call werk-pull card=4654 atomic=true" "$T/calls"
  grep -q "nudge kade .*human-task complete card --run-id run-1 --step skill_werk_code --input by=kade --input decision=done" "$T/calls"
}

@test "negative proof: a refused werk-pull fails the step and tells nobody to code" {
  STUB_RUNNER_FAIL=1 run "$WC" pull 4654 kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"werk-pull refused"* ]] || return 1
  ! grep -q "^nudge" "$T/calls" || return 1
}

@test "check: the card's role saying done passes" {
  run "$WC" check 4654 kade kade done
  [ "$status" -eq 0 ]
}

@test "negative proof: done from another role fails" {
  run "$WC" check 4654 kade silas done
  [ "$status" -eq 1 ]
  [[ "$output" == *"done must come from kade"* ]] || return 1
}

@test "negative proof: stop ends the run, from the role or from Jeff" {
  run "$WC" check 4654 kade kade stop
  [ "$status" -eq 1 ]
  run "$WC" check 4654 kade jeff stop
  [ "$status" -eq 1 ]
  [[ "$output" == *"jeff ended the run"* ]] || return 1
}

@test "negative proof: an answer that is not done or stop fails" {
  run "$WC" check 4654 kade kade maybe
  [ "$status" -eq 1 ]
}

@test "handoff: a card with no model change starts cicd, with no commit first" {
  werk_with 4654 platform/scripts/x.sh
  run "$WC" handoff 4654 kade
  [ "$status" -eq 0 ]
  grep -q "start cicd 4654 kade" "$T/calls"
  ! grep -q "werk-commit" "$T/calls" || return 1
}

@test "handoff: a card that changes the model is committed, then starts athena" {
  werk_with 4654 roles/kade/ontology/x.ttl
  run "$WC" handoff 4654 kade
  [ "$status" -eq 0 ]
  grep -q "runner ROLE=kade call werk-commit card=4654 atomic=true" "$T/calls"
  grep -q "start athena 4654 kade" "$T/calls"
  ! grep -q "start cicd" "$T/calls" || return 1
}

@test "negative proof: a card with no werk is refused, and nothing starts" {
  run "$WC" handoff 4654 kade
  [ "$status" -eq 1 ]
  [ ! -f "$T/calls" ] || ! grep -q "^start" "$T/calls"
}
