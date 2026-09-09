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
#
# 2026-09-08 (#4119): the owner column here reads |unowned|, not |silas|. These
# are FIXTURE paths — fake-crate, platform/api, platform/tests/x.bats — and
# #4113's path rule deliberately answers "unowned" for anything a path cannot
# decide rather than defaulting a teammate's name onto it. What this file grades
# is NEVER-RAN row generation; the owner column is whatever the one owner rule
# says, and pinning a name here would re-import the guess #4113 removed.

# --- how these assertions are written, and why they are not `[[ ]]` ---------
# Measured 2026-09-08 on bats-core 1.13.0 / this bash: a failing `[[ ]]` does
# NOT trip errexit, so an intermediate `[[ ]]` inside a @test is never graded —
# only the LAST command's status decides ok/not ok. One-line proof:
#   bash -c 'set -e; [[ a == b ]]; echo REACHED'   # prints REACHED, exits 0
# Test 1 in this very file was reporting ok while asserting |silas| against
# output that said |unowned|; the trailing "killed at the lane cap" assertion
# was the only one being read. `grep -qF` is a simple command and does trip
# errexit (verified alongside), so every assertion below is actually graded.
has() { grep -qF -- "$2" <<<"$1"; }
lacks() { ! grep -qF -- "$2" <<<"$1"; }

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
  has "$output" "SUITE|cargo|platform/services/fake-crate|unowned|pass|3 pass, 0 fail"
  has "$output" "SUITE|npm|platform/api|unowned|fail|0 pass, 1 fail (NEVER RAN"
  has "$output" "SUITE|bats|platform/tests/x.bats|unowned|fail|0 pass, 1 fail (NEVER RAN"
  # the reason names the cap, so the morning read does not re-diagnose
  has "$output" "killed at the lane cap"
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
  lacks "$output" "NEVER RAN"
  has "$output" "SUITE|npm|platform/api|unowned|fail|1 pass, 2 fail"
}

@test "kinds must match: a security plan is not satisfied by an npm unit line" {
  out=$'nightly-plan|security|platform/api\nnightly-unit|npm|platform/api|pass|1 pass, 0 fail'
  run bash -c "source '$SCRIPT'; _never_ran_rows \"\$1\" 0" _ "$out"
  has "$output" "SUITE|security|platform/api|unowned|fail|0 pass, 1 fail (NEVER RAN"
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
  has "$output" "SUITE|reconcile|tests-domain|kade|fail|0 pass, 1 fail (783 registered test(s) never ran of 7896"
  lacks "$output" "reconciler not found"
}

@test "control: no werk-test anywhere → the census is UNMEASURED, not a fabricated pass" {
  export HOME="$TMP/home-empty"; mkdir -p "$HOME"
  unset NIGHTLY_RECONCILE_BIN
  run env PATH="/usr/bin:/bin" bash -c "source '$SCRIPT'; _reconcile_leg"
  has "$output" "|unmeasured|"
  has "$output" "reconciler not found"
}
