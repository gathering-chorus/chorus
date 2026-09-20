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
  # #4227 — the verb answer is a fixture too. athena-model is not a service;
  # it is owned when the werk built its own copy of the binary. Pointing
  # CHORUS_WERK_BASE at an empty tmp dir keeps that answer out of this box's
  # real bin slot, which already holds one and would make every reading 5.
  WERKBASE="$BATS_TEST_TMPDIR/werk"
  mkdir -p "$WERKBASE/kade-bin"
}

run_fitness() {
  DEMO_FITNESS_LAUNCHCTL="$1" DEMO_FITNESS_SERIES="$SERIES" \
    CHORUS_WERK_BASE="$WERKBASE" "$BIN" kade
}

# The same run with the verb binary present in the werk's bin slot.
run_fitness_with_verb() {
  cp "$BIN" "$WERKBASE/kade-bin/athena-model"
  DEMO_FITNESS_LAUNCHCTL="$1" DEMO_FITNESS_SERIES="$SERIES" \
    CHORUS_WERK_BASE="$WERKBASE" "$BIN" kade
  rm -f "$WERKBASE/kade-bin/athena-model"
}

@test "#4225 counts what the variant started and what it borrowed from prod" {
  run run_fitness "$BATS_TEST_TMPDIR/lc-full"
  echo "$output"
  printf '%s' "$output" | grep -q "4 of 6 target pieces"
  printf '%s' "$output" | grep -q "MISSING : athena-model chorus-hooks"
  printf '%s' "$output" | grep -q "shared  : 3 prod service"
}

@test "#4225 NEGATIVE PROOF: stop one variant service and the count drops" {
  run run_fitness "$BATS_TEST_TMPDIR/lc-degraded"
  echo "$output"
  printf '%s' "$output" | grep -q "3 of 6 target pieces"
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

@test "#4227 a verb counts by its binary, and the same run without it does not" {
  run run_fitness_with_verb "$BATS_TEST_TMPDIR/lc-full"
  echo "$output"
  printf '%s' "$output" | grep -q "5 of 6 target pieces"
  printf '%s' "$output" | grep -q "own     : chorus-api chorus-mcp athena-make athena-model"
  # NEGATIVE PROOF: identical launchctl output, binary removed, count drops.
  run run_fitness "$BATS_TEST_TMPDIR/lc-full"
  echo "$output"
  printf '%s' "$output" | grep -q "4 of 6 target pieces"
  printf '%s' "$output" | grep -q "MISSING : athena-model chorus-hooks"
}
