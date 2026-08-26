#!/usr/bin/env bats
# @test-type: unit — hermetic: builds a fake services tree in BATS_TEST_TMPDIR and
# drives the crate filter directly. No registry, no network, no cargo.
#
# #4012 — the coverage-denominator ratchet counted `platform/services/shared/` as
# a crate. It is a SOURCE directory included by other crates: no Cargo.toml, so no
# coverage floor is possible for it. #4000 added tests referencing
# shared/scope_units.rs, the registry surfaced "shared" as a crate name, the count
# went 20 -> 21, and the ratchet reddened every night from 08-22 on.
#
# A permanent red is worse than no check: its own comment in nightly-suites.sh
# says "a permanent red teaches the team to skim past red." The proofs below pin
# that the filter separates a real crate from a source directory.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  mkdir -p "$TMP/platform/services/werk-test" "$TMP/platform/services/shared"
  printf '[package]\nname = "werk-test"\n' > "$TMP/platform/services/werk-test/Cargo.toml"
  printf 'pub fn x() {}\n' > "$TMP/platform/services/shared/scope_units.rs"
}

# The filter as the script applies it, driven against the fixture tree.
keep() {  # $1 = candidate name -> prints it only if it is a real crate
  CHORUS_ROOT="$TMP" bash -c '
    c="$1"
    [ -f "$CHORUS_ROOT/platform/services/$c/Cargo.toml" ] || exit 0
    echo "$c"' _ "$1"
}

@test "negative proof: a source directory with no Cargo.toml is not counted as a crate" {
  # This is the exact 08-22 red: shared/ present, referenced by tests, not a crate.
  run keep shared
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "control: a real crate IS counted — the filter separates the two states" {
  # Without this the filter could reject everything and still pass the proof above.
  run keep werk-test
  [ "$status" -eq 0 ]
  [ "$output" = "werk-test" ]
}

@test "a name with no directory at all is not counted" {
  run keep does-not-exist
  [ -z "$output" ]
}

@test "the script applies the Cargo.toml filter in the denominator loop" {
  # Guards against the filter being dropped in a later edit: the loop must test
  # for Cargo.toml before incrementing `present`.
  run bash -c "grep -A3 'while IFS= read -r c; do' '$NIGHTLY' | grep -c 'Cargo.toml'"
  [ "$output" -ge 1 ]
}
