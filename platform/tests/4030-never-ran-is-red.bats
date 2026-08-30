#!/usr/bin/env bats
# @test-type: unit — hermetic; werk-test is a stub that prints a plan and part
# of it, chorus-log is a stub, no live server, no real suites.
#
# #4030 AC4 — a suite the run PLANNED and never REACHED is red, not absent.
# 2026-08-30 03:00: the npm lane hung on platform/api, the 7200s lane cap
# killed the runner, five npm packages and every bats suite never ran, and the
# morning nudge said "3 red" — counting only the units the run got to.
# Negative proof (#3734): the violating state (a plan the run did not finish)
# is shown to produce red rows; the control (every planned unit reported)
# produces none, so the check can tell the two states apart.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_FAIL_DIR="$TMP/failures"
  export CHORUS_LOG_BIN="$TMP/chorus-log"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
  BIN="$TMP/bin"; mkdir -p "$BIN"
  export NIGHTLY_LOAD_STUB=0.1
}

@test "negative proof: a runner killed mid-plan yields a NEVER RAN fail row per unreached unit" {
  cat > "$BIN/werk-test" <<EOF
#!/usr/bin/env bash
echo "nightly-plan|cargo|fake-crate"
echo "nightly-plan|npm|platform/api"
echo "nightly-plan|bats|platform/tests/x.bats"
echo "nightly-unit|cargo|fake-crate|pass|3 pass, 0 fail"
exit 124
EOF
  chmod +x "$BIN/werk-test"
  PATH="$BIN:$PATH" run "$SCRIPT" --run-one cargo fake-crate
  [ "$status" -eq 1 ]
  [[ "$output" == *"SUITE|cargo|platform/services/fake-crate|silas|pass|3 pass, 0 fail"* ]]
  [[ "$output" == *"SUITE|npm|platform/api|silas|fail|0 pass, 1 fail (NEVER RAN"* ]]
  [[ "$output" == *"SUITE|bats|platform/tests/x.bats|silas|fail|0 pass, 1 fail (NEVER RAN"* ]]
  # the reason names the cap, so the morning read does not re-diagnose
  [[ "$output" == *"killed at the lane cap"* ]]
}

@test "control: every planned unit reported → no NEVER RAN row" {
  cat > "$BIN/werk-test" <<EOF
#!/usr/bin/env bash
echo "nightly-plan|cargo|fake-crate"
echo "nightly-plan|npm|platform/api"
echo "nightly-unit|cargo|fake-crate|pass|3 pass, 0 fail"
echo "nightly-unit|npm|platform/api|fail|1 pass, 2 fail"
exit 1
EOF
  chmod +x "$BIN/werk-test"
  PATH="$BIN:$PATH" run "$SCRIPT" --run-one cargo fake-crate
  [[ "$output" != *"NEVER RAN"* ]]
  [[ "$output" == *"SUITE|npm|platform/api|silas|fail|1 pass, 2 fail"* ]]
}

@test "kinds must match: a security plan is not satisfied by an npm unit line" {
  out=$'nightly-plan|security|platform/api\nnightly-unit|npm|platform/api|pass|1 pass, 0 fail'
  run bash -c "source '$SCRIPT'; _never_ran_rows \"\$1\" 0" _ "$out"
  [[ "$output" == *"SUITE|security|platform/api|silas|fail|0 pass, 1 fail (NEVER RAN"* ]]
}

@test "a runner that prints no plan (older binary) adds no rows — never a fabricated red" {
  out=$'nightly-unit|cargo|fake-crate|pass|3 pass, 0 fail'
  run bash -c "source '$SCRIPT'; _never_ran_rows \"\$1\" 124" _ "$out"
  [ -z "$output" ]
}

# The 03:00 census said "reconciler not found" every night since the runner
# moved to ~/.chorus/bin (#2734): launchd's PATH predates it, so never-ran
# read as UNMEASURED, never as red. The runner lane already fell back to
# $HOME/.chorus/bin; the census leg now does too.
@test "the census finds werk-test in ~/.chorus/bin when PATH does not carry it" {
  export HOME="$TMP/home"; mkdir -p "$HOME/.chorus/bin"
  cat > "$HOME/.chorus/bin/werk-test" <<'EOF'
#!/usr/bin/env bash
echo "reconcile: registered 7896, never-run (783):"
exit 0
EOF
  chmod +x "$HOME/.chorus/bin/werk-test"
  unset NIGHTLY_RECONCILE_BIN
  run env PATH="/usr/bin:/bin" bash -c "source '$SCRIPT'; _reconcile_leg"
  [[ "$output" == *"SUITE|reconcile|tests-domain|kade|fail|0 pass, 1 fail (783 registered test(s) never ran of 7896"* ]]
  [[ "$output" != *"reconciler not found"* ]]
}

@test "control: no werk-test anywhere → the census is UNMEASURED, not a fabricated pass" {
  export HOME="$TMP/home-empty"; mkdir -p "$HOME"
  unset NIGHTLY_RECONCILE_BIN
  run env PATH="/usr/bin:/bin" bash -c "source '$SCRIPT'; _reconcile_leg"
  [[ "$output" == *"|unmeasured|"* ]]
  [[ "$output" == *"reconciler not found"* ]]
}
