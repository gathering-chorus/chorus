#!/usr/bin/env bats
# @test-type: contract
# #4173 — the already-landed accept gate must read JEFF'S GO, not its own output.
#
# werk.yml's already-landed retry grepped the witness for "demo.decision".
# werk-accept is the only writer of demo.decision (werk-accept/src/lib.rs:86),
# so accept required a record that only accept produces. Any card whose content
# reached main before accept was unacceptable forever: #4175 was merged,
# deployed and live, with Jeff's go on the witness three times, and stuck WIP.

setup() {
  # The tree this test SHIPS IN, never CHORUS_ROOT. The runner sets CHORUS_ROOT
  # to canonical, so a werk copy of this suite was reading canonical's werk.yml
  # — the pre-fix file — and reporting the werk's own change as absent. A test
  # that reads a file must read the one it was committed beside.
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  YML="$REPO/.github/workflows/werk.yml"
  WITNESS="$BATS_TEST_TMPDIR/werk-demo.jsonl"
  # The grep werk.yml runs, with the event name read out of the workflow itself
  # so this test cannot pass against a file that no longer says what it claims.
  EVENT=$(grep -o '"event\\":\\"demo\.[a-z]*\\",\\"role' "$YML" | head -1 | sed 's/.*demo\.\([a-z]*\).*/demo.\1/')
}

gate() {   # gate <card_id> — the workflow's own condition, run against $WITNESS
  grep -q "\"event\":\"$EVENT\",\"role\":\"[a-z]*\",\"card_id\":$1," "$WITNESS" 2>/dev/null
}

@test "the already-landed gate reads the human go" {
  [ "$EVENT" = "demo.go" ]
}

@test "a card with Jeff's go recorded is accepted" {
  echo '{"ts":1,"event":"demo.go","role":"wren","card_id":4175,"trace_id":"t"}' > "$WITNESS"
  run gate 4175
  [ "$status" -eq 0 ]
}

# NEGATIVE PROOF (#3734): the gate must still REFUSE a card with no human go,
# or reading a different event has only replaced one hollow check with another.
# The second case is the sharp one — demo.decision alone must not pass, because
# accept writing its own permission slip is exactly the #3410 self-accept hole.
@test "NEGATIVE PROOF: no go, and accept's own record, both refuse" {
  : > "$WITNESS"
  run gate 4175
  [ "$status" -ne 0 ]

  echo '{"ts":1,"event":"demo.decision","role":"wren","card_id":4175,"trace_id":"t","decision":"go"}' > "$WITNESS"
  run gate 4175
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: another card's go does not accept this one" {
  echo '{"ts":1,"event":"demo.go","role":"kade","card_id":4173,"trace_id":"t"}' > "$WITNESS"
  run gate 4175
  [ "$status" -ne 0 ]
}
