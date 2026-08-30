#!/usr/bin/env bats
# @test-type: unit — hermetic: stubs werk-test + chorus-log; no live service, brings its own world.
# #4032 — the runner (werk-test --nightly) mints its write credential from
# $CHORUS_HOME. launchd never sets it, so every 03:00 run since #4015 stored
# nothing while the same binary from a role shell stored 6,120. The lane must
# HAND the runner CHORUS_HOME; a launchd-shaped environment (no CHORUS_HOME)
# is the fixture, and the negative proof runs the pre-#4032 invocation shape
# through it to show the runner sees nothing.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  export CHORUS_LOG_BIN="$TMP/chorus-log"
  printf '#!/bin/bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
  export NIGHTLY_LOAD_STUB=0.1
  export NIGHTLY_CARGO_TARGET="$TMP/target"
  # A stand-in runner: reports the one thing the real one needs, then emits a
  # unit line so the lane folds it normally.
  mkdir -p "$TMP/bin"
  # The lane captures the runner's stdout into its own file, so the stub
  # reports through a side file ($NIGHTLY_STUB_OUT) the tests read back.
  export NIGHTLY_STUB_OUT="$TMP/seen.txt"
  cat > "$TMP/bin/werk-test" <<'STUB'
#!/bin/bash
{
  echo "runner-sees CHORUS_HOME=${CHORUS_HOME:-<unset>}"
  if [ -n "$CHORUS_HOME" ] && [ -x "$CHORUS_HOME/platform/scripts/chorus-identity-token" ]; then
    echo "runner-can-mint yes"
  else
    echo "runner-can-mint no"
  fi
} | tee -a "$NIGHTLY_STUB_OUT"
echo "nightly-unit|bats|platform/tests/stub.bats|pass|1 pass, 0 fail"
STUB
  chmod +x "$TMP/bin/werk-test"
  export PATH="$TMP/bin:$PATH"
  export CHORUS_ROOT="$BATS_TEST_DIRNAME/../.."
}

@test "negative proof: the pre-#4032 invocation under a launchd-shaped env leaves the runner blind (cannot mint)" {
  # The exact shape the lane used before this card: CHORUS_ROOT handed over,
  # CHORUS_HOME not. From an env with no CHORUS_HOME, the runner sees nothing.
  run env -u CHORUS_HOME bash -c "env CARGO_TARGET_DIR='$TMP/target' CHORUS_ROOT='$CHORUS_ROOT' werk-test --nightly"
  [ "$status" -eq 0 ]
  [[ "$output" == *"runner-sees CHORUS_HOME=<unset>"* ]]
  [[ "$output" == *"runner-can-mint no"* ]]
}

@test "with the fix, the lane hands the runner CHORUS_HOME even when launchd did not set it" {
  run env -u CHORUS_HOME bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    run_cargo_lane >/dev/null 2>&1"
  seen=$(cat "$NIGHTLY_STUB_OUT")
  [[ "$seen" == *"runner-sees CHORUS_HOME=$CHORUS_ROOT"* ]]
  [[ "$seen" == *"runner-can-mint yes"* ]]
}

@test "control: a CHORUS_HOME already in the environment is honoured, not overwritten" {
  run env CHORUS_HOME="$TMP/elsewhere" bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    run_cargo_lane >/dev/null 2>&1"
  seen=$(cat "$NIGHTLY_STUB_OUT")
  [[ "$seen" == *"runner-sees CHORUS_HOME=$TMP/elsewhere"* ]]
}

@test "the LaunchAgent plist carries CHORUS_HOME so a kickstart is not a different world from a shell" {
  grep -q '<key>CHORUS_HOME</key>' "$BATS_TEST_DIRNAME/../scripts/com.chorus.nightly-suites.plist"
}
