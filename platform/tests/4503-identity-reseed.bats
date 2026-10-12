#!/usr/bin/env bats
# @test-type: unit:security — runs the script as the test user; nothing is copied, no Mac account touched
# @domain: identity
# @card: #4503
#
# After `chorus-principal move <role> --apply`, Jeff copies the role's new login
# into its own Mac account with `sudo chorus-identity-reseed <role>`. These
# prove it refuses everything except a plain role name run as root.
#
# Covers: platform/scripts/chorus-identity-reseed

setup() {
  RESEED="${BATS_TEST_DIRNAME}/../scripts/chorus-identity-reseed"
}

@test "no role: usage, exit 2" {
  run bash "$RESEED"
  [ "$status" -eq 2 ]
  [[ "$output" == *"usage: sudo chorus-identity-reseed <role>"* ]] || return 1
}

@test "a name with a slash or .. is refused before anything else" {
  for bad in ../jeff a/b ..; do
    run bash "$RESEED" "$bad"
    [ "$status" -eq 2 ] || { echo "$bad passed"; return 1; }
    [[ "$output" == *"usage:"* ]] || { echo "$bad: $output"; return 1; }
  done
}

@test "not root: refused, names sudo, writes nothing" {
  [ "$(id -u)" -ne 0 ] || skip "the suite runs as a normal user"
  run bash "$RESEED" silas
  [ "$status" -eq 2 ]
  [[ "$output" == *"run with sudo"* ]] || return 1
}

@test "NEGATIVE PROOF: without the root check the script goes on to copy, so the refusal above is the check doing it" {
  sed '/id -u)" -eq 0/d' "$RESEED" > "$BATS_TEST_TMPDIR/reseed"
  if grep -q 'id -u)" -eq 0' "$BATS_TEST_TMPDIR/reseed"; then echo "mutation did not apply"; return 1; fi
  run bash "$BATS_TEST_TMPDIR/reseed" zzz-no-such-role
  [ "$status" -eq 2 ]
  [[ "$output" != *"run with sudo"* ]] || return 1
  [[ "$output" == *"nothing copied"* ]] || return 1
}
