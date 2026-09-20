#!/usr/bin/env bats
# @test-type: integration — drives the built demo-fitness binary with a stub launchctl; no services touched
# #4225 — the demo-fitness line, driven with a stub launchctl so the numbers are
# fixtures rather than whatever this box happens to be running: a check whose
# verdict depends on the machine cannot be read as red or green.
#
# Asserts are simple commands — `[[ ]]` mid-block and `! cmd` under set -e both
# pass whatever they claim on bash 3.2 (#4213).

setup() {
  BIN="${DEMO_FITNESS_BIN:-$(cd "$BATS_TEST_DIRNAME/../services/demo-fitness" && pwd)/target/debug/demo-fitness}"
  [ -x "$BIN" ] || skip "demo-fitness not built at $BIN"
  SERIES="$BATS_TEST_TMPDIR/series.jsonl"
  printf '%s\n' '#!/bin/bash' 'cat <<LIST' \
    '830	0	com.chorus.pulse' \
    '64992	0	com.chorus.api' \
    '3295	0	com.chorus.athena-make' \
    '95309	0	com.chorus.api.werk.kade' \
    '95510	0	com.chorus.mcp.werk.kade' \
    '95583	0	com.chorus.athena-make.werk.kade' \
    '1335	0	com.chorus.clearing.werk.kade' \
    '-	0	com.chorus.cruft-scan' \
    'LIST' > "$BATS_TEST_TMPDIR/lc-full"
  grep -v "clearing.werk.kade" "$BATS_TEST_TMPDIR/lc-full" > "$BATS_TEST_TMPDIR/lc-degraded"
  chmod +x "$BATS_TEST_TMPDIR/lc-full" "$BATS_TEST_TMPDIR/lc-degraded"
}

run_fitness() {
  DEMO_FITNESS_LAUNCHCTL="$1" DEMO_FITNESS_SERIES="$SERIES" "$BIN" kade
}

@test "#4225 counts what the variant started and what it borrowed from prod" {
  run run_fitness "$BATS_TEST_TMPDIR/lc-full"
  echo "$output"
  printf '%s' "$output" | grep -q "4 of 6 target services"
  printf '%s' "$output" | grep -q "MISSING : athena-model chorus-hooks"
  printf '%s' "$output" | grep -q "shared  : 3 prod service"
}

@test "#4225 NEGATIVE PROOF: stop one variant service and the count drops" {
  run run_fitness "$BATS_TEST_TMPDIR/lc-degraded"
  echo "$output"
  printf '%s' "$output" | grep -q "3 of 6 target services"
  printf '%s' "$output" | grep -q "MISSING : athena-model chorus-hooks clearing"
  test -z "$(printf '%s' "$output" | grep -F '4 of 6' || true)"
}

@test "#4225 the series is append-only, so a run can be read against the last" {
  run_fitness "$BATS_TEST_TMPDIR/lc-full" >/dev/null
  run_fitness "$BATS_TEST_TMPDIR/lc-degraded" >/dev/null
  test "$(grep -c . "$SERIES")" -eq 2
  run run_fitness "$BATS_TEST_TMPDIR/lc-full"
  printf '%s' "$output" | grep -q "(prev 3)"
}
