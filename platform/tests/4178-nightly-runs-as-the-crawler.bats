#!/usr/bin/env bats
# @test-type: contract
# #4178 — the scheduled pass must present the automation identity.
#
# The door stamps ownedBy from the caller and only the owner may update a row,
# so whoever runs the crawler TAKES every row it writes. Wired as a person, the
# first nightly reassigns ~5,500 rows away from the automation and every run
# after it is refused. Measured on the variant 2026-09-15: one pass run as
# `wren` left {crawler 5547, wren 1}.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PLIST="$REPO/platform/launchd/com.chorus.crawl-nightly.plist"
}

@test "the nightly unit exists and is valid" {
  [ -f "$PLIST" ]
  run plutil -lint "$PLIST"
  [ "$status" -eq 0 ]
}

@test "it runs as the crawler, not as a person" {
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_ROLE' "$PLIST"
  [ "$status" -eq 0 ]
  [ "$output" = "crawler" ]
}

# NEGATIVE PROOF (#3734): the check must FAIL on the state it exists to catch —
# the same unit wired to a person's identity. Without this it would pass on any
# plist that merely has the key.
@test "NEGATIVE PROOF: a person's identity in this unit is caught" {
  bad="$BATS_TEST_TMPDIR/bad.plist"
  cp "$PLIST" "$bad"
  /usr/libexec/PlistBuddy -c 'Set :EnvironmentVariables:CHORUS_ROLE kade' "$bad"
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_ROLE' "$bad"
  [ "$output" != "crawler" ]
  [ "$output" = "kade" ]
}
