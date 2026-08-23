#!/usr/bin/env bats
# @test-type: unit — hermetic; stubs werk-test on PATH, asserts target-dir isolation, no real build
# #3484 — the nightly cargo lane must run in an ISOLATED CARGO_TARGET_DIR so it
# can never contend with a role/recovery `cargo` over a crate's shared target/
# build lock (2026-06-20: cross-process lock contention painted every crate red
# at once while each was green standalone). A private target dir = the
# nightly's own lock.
#
# Rewritten for #3974's architecture on #3753: run_one_attempt was retired —
# the whole lane now routes through `werk-test --nightly` (run_cargo_lane), so
# the isolation contract is asserted on the env the runner is handed. The old
# tests sourced the retired function and have been red since #3974 landed.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  BIN="$TMP/bin"; mkdir -p "$BIN"
  CAP="$TMP/target-capture.txt"
  # stub werk-test: record the CARGO_TARGET_DIR it was given, emit one green
  # nightly-unit line in the runner's fold format
  cat > "$BIN/werk-test" <<EOF
#!/usr/bin/env bash
echo "\${CARGO_TARGET_DIR:-UNSET}" >> "$CAP"
echo "nightly-unit|cargo|fake-crate|pass|1 pass, 0 fail"
exit 0
EOF
  chmod +x "$BIN/werk-test"
  export NIGHTLY_CARGO_TARGET="$TMP/nt"
  export PATH="$BIN:$PATH"
  export NIGHTLY_LOAD_STUB=0.1   # #3753: gate must not interfere with this contract
}

@test "nightly runner lane gets an isolated CARGO_TARGET_DIR, not the crate's shared target/" {
  run "$SCRIPT" --run-one cargo fake-crate
  [ "$status" -eq 0 ]
  run cat "$CAP"
  [[ "$output" == *"$TMP/nt"* ]]
  [[ "$output" != *"UNSET"* ]]
}

@test "isolated run still reports the crate's real result (a green crate is GREEN)" {
  run "$SCRIPT" --run-one cargo fake-crate
  [ "$status" -eq 0 ]
  [[ "$output" == *"|pass|"* ]]
  [[ "$output" == *"1 pass"* ]]
}
