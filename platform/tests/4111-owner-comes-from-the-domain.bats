#!/usr/bin/env bats
# @test-type: unit — hermetic. Feeds owner_for a fixture registry served from a
# file:// map; no live owl-api, no network.
#
# #4111 — Jeff, 2026-09-06: "doesnt the domain imply the owner". It does, and
# the model has held the answer all along: all 7,927 registered tests carry a
# `covers` domain and all 40 Domain rows carry `ownedBy`. The nightly ignored
# both and attributed by directory, so every `platform/**` suite read as
# Silas's — including Kade's own 4106 registry test, which sent Silas to fix a
# test he did not write.

setup() {
  NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  export NIGHTLY_OWNER_MAP="$BATS_TEST_TMPDIR/map"
}

owner() {
  bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; owner_for '$1'" 2>/dev/null
}

@test "the model's answer wins over the directory" {
  printf '%s\t%s\n' "platform/tests/a.bats" "kade" "platform/tests/b.bats" "wren" > "$NIGHTLY_OWNER_MAP"
  [ "$(owner platform/tests/a.bats)" = "kade" ]
  [ "$(owner platform/tests/b.bats)" = "wren" ]
}

@test "negative proof: without the map the same file reads as the directory's owner" {
  # The state the report was in every morning. If the map is ever silently
  # dropped, this is what comes back — so the two must not look alike.
  : > "$NIGHTLY_OWNER_MAP"
  [ "$(owner platform/tests/a.bats)" = "silas" ]
}

@test "a file the registry does not know falls back, it does not go blank" {
  printf '%s\t%s\n' "platform/tests/a.bats" "kade" > "$NIGHTLY_OWNER_MAP"
  run owner platform/tests/unknown.bats
  [ -n "$output" ]
  [ "$output" = "silas" ]
}

@test "an absolute path resolves the same as its repo-relative form" {
  printf '%s\t%s\n' "platform/tests/a.bats" "kade" > "$NIGHTLY_OWNER_MAP"
  [ "$(owner "$CHORUS_ROOT/platform/tests/a.bats")" = "kade" ]
}

@test "negative proof: a prefix of a mapped path does not inherit its owner" {
  # awk matches the whole field, not a prefix — "platform/tests/a.bats.bak"
  # must not read as kade just because it starts with a mapped name.
  printf '%s\t%s\n' "platform/tests/a.bats" "kade" > "$NIGHTLY_OWNER_MAP"
  [ "$(owner platform/tests/a.bats.bak)" = "silas" ]
}
