#!/usr/bin/env bats
# @test-type: unit — hermetic: fixture nightly log + stubbed ops-nudge/chorus-log; no real I/O
# #3999 — daily-review must label from parseable results; "BUILD BROKE" only when
# there is truly no test output. Positive fixtures are the EXACT captured lines
# from the 2026-08-24 06:05 fourth-strike mislabel (never hand-typed).

setup() {
  REVIEW="$BATS_TEST_DIRNAME/../scripts/daily-review-quality.sh"
  F="$BATS_TEST_TMPDIR/nightly.log"
  # stub the side-effect commands the script resolves via its SCRIPT_DIR
  STUB="$BATS_TEST_TMPDIR/stub-scripts"
  mkdir -p "$STUB"
  cp "$REVIEW" "$STUB/daily-review-quality.sh"
  cp "$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh" "$STUB/nightly-suites.sh"
  printf '#!/bin/sh\necho "$@" >> "$BATS_TEST_TMPDIR/nudges.txt"\nexit 0\n' > "$STUB/ops-nudge"; chmod +x "$STUB/ops-nudge"
  printf '#!/bin/sh\nexit 0\n' > "$STUB/chorus-log"; chmod +x "$STUB/chorus-log"
}

run_review() {
  NIGHTLY_LOG_PATH="$F" CHORUS_ROOT="$BATS_TEST_DIRNAME/../.." bash "$STUB/daily-review-quality.sh" 2>/dev/null
}

@test "captured cargo line '652 pass, 2 fail' labels as counts, never BUILD BROKE" {
  printf 'SUITE|cargo|platform/services/chorus-hooks|silas|fail|652 pass, 2 fail\n' > "$F"
  run run_review
  N="$(cat "$BATS_TEST_TMPDIR/nudges.txt" 2>/dev/null || true)"
  [[ "$output$N" != *"BUILD BROKE"* ]]
  [[ "$N" == *"2/654 failed"* ]]
}

@test "captured npm lines (cards 612/4, clearing 800/22) label as counts" {
  printf 'SUITE|npm|directing/products/cards|kade|fail|612 pass, 4 fail\nSUITE|npm|directing/clearing|wren|fail|800 pass, 22 fail\n' > "$F"
  run run_review
  N="$(cat "$BATS_TEST_TMPDIR/nudges.txt" 2>/dev/null || true)"
  [[ "$output$N" != *"BUILD BROKE"* ]]
}

@test "NEGATIVE PROOF: a genuinely output-less failure still says BUILD BROKE (#3734)" {
  printf 'SUITE|cargo|platform/services/chorus-hooks|silas|fail|\n' > "$F"
  run run_review
  N="$(cat "$BATS_TEST_TMPDIR/nudges.txt" 2>/dev/null || true)"
  [[ "$N" == *"BUILD BROKE"* ]]
}
