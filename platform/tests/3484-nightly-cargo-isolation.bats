#!/usr/bin/env bats
# @test-type: unit — hermetic; stubs cargo on PATH, asserts target-dir isolation, no real build
# #3484 — the nightly cargo step must run in an ISOLATED CARGO_TARGET_DIR so it
# can never contend with a role/recovery `cargo` over a crate's shared target/
# build lock. That cross-process contention returns nonzero, the cargo synthesis
# stamps "0 ok, 1 failed", and because it hits every crate it paints the whole
# run red at once (2026-06-20: werk-push/owl-api/chorus-model all "red" while
# each was green standalone). A private target dir = the nightly's own lock.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  CRATE="$TMP/fake-crate"; mkdir -p "$CRATE"
  BIN="$TMP/bin"; mkdir -p "$BIN"
  CAP="$TMP/target-capture.txt"
  # stub cargo: record the CARGO_TARGET_DIR it was given, emit a passing result
  cat > "$BIN/cargo" <<EOF
#!/usr/bin/env bash
echo "\${CARGO_TARGET_DIR:-UNSET}" >> "$CAP"
echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
exit 0
EOF
  chmod +x "$BIN/cargo"
}

@test "nightly cargo runs in an isolated CARGO_TARGET_DIR, not the crate's shared target/" {
  NIGHTLY_CARGO_TARGET="$TMP/nt" PATH="$BIN:$PATH" \
    bash -c "source '$SCRIPT'; run_one_attempt cargo '$CRATE' silas" >/dev/null
  run cat "$CAP"
  [[ "$output" == *"$TMP/nt"* ]]
  [[ "$output" != *"UNSET"* ]]
}

@test "isolated build still reports the crate's real result (a green crate is GREEN)" {
  NIGHTLY_CARGO_TARGET="$TMP/nt" PATH="$BIN:$PATH" \
    run bash -c "source '$SCRIPT'; run_one_attempt cargo '$CRATE' silas"
  [[ "$output" == *"|pass|"* ]]
  [[ "$output" == *"1 ok"* ]]
}
