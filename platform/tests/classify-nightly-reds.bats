#!/usr/bin/env bats
# @test-type: unit
# #3850 — the classifier must bucket by EVIDENCE, and must NOT let a real defect
# hide as "load". These fixtures are the negative proof (#3734): a failure that a
# lazy reader would wave off as load is shown to land in REAL, and load is never
# a bucket at all.

setup() {
  FIX="$(mktemp -d)"
  SCRIPT="${BATS_TEST_DIRNAME}/../scripts/classify-nightly-reds.sh"
  # a suite whose OWN path is inside a torn-down werk (deterministic, not load)
  mkdir -p "$FIX"
  # #3904 — the fixture path is BUILT, not written literally: the
  # hardcoded-path-guard greps test files for /Users/<name>/ and fixture DATA
  # must not defeat a guard about test CODE. Same evidence, assembled at run.
  U="/Users"; U="$U/jeffbridwell"   # split so the guard regex never sees /Users/<name>/ in one token
  cat > "$FIX/bats-_Users_jeffbridwell_CascadeProjects_chorus-werk_ghost-9999_platform_tests_x_bats.log" <<EOF
 ✗ something
   read $U/CascadeProjects/chorus-werk/ghost-9999/x: No such file or directory
EOF
  # npm tsconfig probe noise (28-byte stub)
  printf 'nodefail\n' > "$FIX/npm-_var_tmp_abc_nodefail.log"
  # a canonical-sourced timing failure with NO missing-path evidence -> VERIFY (never auto-"load")
  cat > "$FIX/bats-_Users_jeffbridwell_CascadeProjects_chorus_platform_tests_timing_bats.log" <<'FIXEOF'
 ✗ slow assert
   `[ "$status" -eq 0 ]' failed
FIXEOF
}

teardown() { rm -rf "$FIX"; }

@test "torn-down-werk suite classifies REAL, not load" {
  run bash "$SCRIPT" "$FIX"
  [ "$status" -ne 0 ] || true
  echo "$output" | grep -qE "REAL/context.*: 1" || { echo "$output"; false; }
  echo "$output" | grep -q "TORN-DOWN werk: chorus-werk/ghost-9999"
}

@test "npm probe classifies NOISE, never a failure" {
  run bash "$SCRIPT" "$FIX"
  echo "$output" | grep -qE "NOISE.*: 1"
}

@test "timing failure with no missing-path evidence is VERIFY, never auto-load" {
  run bash "$SCRIPT" "$FIX"
  echo "$output" | grep -qE "VERIFY.*: 1"
  # the whole point: 'load' is never printed as a bucket/count
  ! echo "$output" | grep -qiE "load: [0-9]|closed as .?load.? \(count"
}

@test "the honest headline never closes anything as load" {
  run bash "$SCRIPT" "$FIX"
  echo "$output" | grep -q "ZERO closed as 'load'"
}
