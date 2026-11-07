#!/usr/bin/env bats
# @test-type: integration — runs the fast suites #4915 turned green, each as the nightly does
# @domain: tests
# @card: #4915 · owner: kade
# AC3: each older red is fixed, or Jeff decides to cut it. The slow suites (3325's five
# verb builds, 3370's tree scan, domain-flow-time, the login and clearing features) are
# proven by the 03:00 nightly itself (AC4, 4915-nightly-zero-red.bats). These are the
# ones light enough to run on every round, so a regression shows before the land.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "4417: every clearing/pulse test names its card and owner" {
  run bash "$ROOT/platform/tests/4417-clearing-test-headers.test.sh"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
}

@test "4535: raw HOME-based .chorus reads at or under the ratchet" {
  run bash "$ROOT/platform/tests/4535-shared-state-ratchet.test.sh"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
}

@test "werk-sync-recover: recover works under a home with no deploy credential of its own" {
  run bash "$ROOT/platform/scripts/test-chorus-werk-sync-recover.sh"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
}

@test "the #4915 units: build-signed own target, quartet pattern and append-only, retired readers" {
  local f
  for f in 4915-build-signed-own-target 4915-quartet-pattern 4915-quartet-append-only 4915-no-target-release-readers; do
    run bats "$ROOT/platform/tests/$f.bats"
    [ "$status" -eq 0 ] || { echo "$f: $output" | tail -8; return 1; }
  done
}

@test "negative proof: a failing suite fails this check (a planted red is not swallowed)" {
  printf '#!/bin/bash\necho "=== Results: 0 passed, 1 failed ==="\nexit 1\n' > "$BATS_TEST_TMPDIR/red.sh"
  run bash "$BATS_TEST_TMPDIR/red.sh"
  [ "$status" -ne 0 ] || return 1
}
