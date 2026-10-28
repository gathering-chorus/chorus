#!/usr/bin/env bats
# @test-type: unit — #4765 AC3: bridge-subscriber accepts AGY roles (borg-queen, abby-normal)
# @card: 4765 · owner: borg-queen

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO/platform/scripts/bridge-subscriber.js"

@test "negative: unknown role is refused" {
  run node "$SCRIPT" invalid-role-xyz
  [ "$status" -eq 1 ]
  [[ "$output" == *"bad role 'invalid-role-xyz'"* ]] || return 1
}

@test "negative: empty role is refused" {
  run node "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"bad role ''"* ]] || return 1
}

@test "AC3: borg-queen is accepted as valid role" {
  # Should not be refused with bad role error (exits past role check)
  run node "$SCRIPT" borg-queen
  [[ "$output" != *"bad role 'borg-queen'"* ]] || return 1
}

@test "AC3: abby-normal is accepted as valid role" {
  # Should not be refused with bad role error (exits past role check)
  run node "$SCRIPT" abby-normal
  [[ "$output" != *"bad role 'abby-normal'"* ]] || return 1
}
