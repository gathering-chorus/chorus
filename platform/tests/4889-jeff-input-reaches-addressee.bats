#!/usr/bin/env bats
# @test-type: integration — Clearing v2's real app on a local port with a stand-in pulse
# @domain: messages
# @card: #4889 · owner: silas
# What Jeff sees: a message he writes in a role's room reaches that role and no other,
# agy roles included. Jeff 11-05 11:14: his 11-04 18:27 post in @ borg-queen landed as
# messages.db row 67493 with to: wren — directing/clearing-v2/public/app.js signed every
# post room 'team', so src/router.ts handed it to the moderator. Proven in
# directing/clearing-v2/tests/jeff-input-addressee-4889.test.ts (negative proof: the old
# 'team' signing lands with to: wren).
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CV2="$ROOT/directing/clearing-v2"

@test "a post in the borg-queen or abby-normal room reaches that role alone; signed 'team' it lands with wren" {
  run bash -c "cd '$CV2' && npx jest tests/jeff-input-addressee-4889.test.ts --silent 2>&1"
  [ "$status" -eq 0 ] || { echo "$output" | tail -40; return 1; }
  [[ "$output" == *"Tests:"*"4 passed"* ]] || { echo "the four cases did not all run: $output" | tail -10; return 1; }
}

@test "the composer has no hard-coded room left" {
  ! grep -n "\['room', 'team'\]\]\|{ room: 'team' }" "$CV2/public/app.js" || return 1
}
