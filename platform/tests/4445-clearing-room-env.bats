#!/usr/bin/env bats
# @test-type: unit — runs platform/scripts/clearing-room-env with a temp secret file; no relay, no Clearing.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4445 — a demo variant Clearing turns its room on the way prod does: the room
# secret is read from a 600 file at start, never from the (644) plist.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  RUNNER="$ROOT/platform/scripts/clearing-room-env"
  T="$BATS_TEST_TMPDIR"
  printf 'fixture-room-secret' > "$T/room-secret"; chmod 600 "$T/room-secret"
  unset BUZZ_ROOM_SECRET BUZZ_ROOM_ENABLED BUZZ_RELAY_HOST_HEADER
}

@test "with a readable secret the command runs with the room on" {
  run env BUZZ_ROOM_SECRET_FILE="$T/room-secret" "$RUNNER" bash -c 'echo "on=$BUZZ_ROOM_ENABLED host=${BUZZ_RELAY_HOST_HEADER:-none} len=${#BUZZ_ROOM_SECRET}"'
  test "$status" -eq 0
  # #4445 reopen 1 — the runner pins no relay address; dialing the relay sends its own host
  printf '%s' "$output" | grep -qF "on=1 host=none len=19"
}

@test "a caller's relay Host header passes through untouched" {
  run env BUZZ_ROOM_SECRET_FILE="$T/room-secret" BUZZ_RELAY_HOST_HEADER="relay.test:3000" "$RUNNER" bash -c 'echo "host=$BUZZ_RELAY_HOST_HEADER"'
  test "$status" -eq 0
  printf '%s' "$output" | grep -qF "host=relay.test:3000"
}

@test "NEGATIVE PROOF: no readable secret, the command still runs with the room off and says so" {
  run env BUZZ_ROOM_SECRET_FILE="$T/missing" "$RUNNER" bash -c 'echo "on=${BUZZ_ROOM_ENABLED:-0}"'
  test "$status" -eq 0
  printf '%s' "$output" | grep -qF "on=0"
  printf '%s' "$output" | grep -qF "the room stays off"
}
