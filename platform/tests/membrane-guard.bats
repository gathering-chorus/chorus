#!/usr/bin/env bats
# @test-type: integration — spawns the built shim binary, but hermetic-mode: redirected HOME, touches no live state
load test_helper
# membrane-guard.bats — #3615 test/prod membrane, end-to-end negative proof.
#
# The #3734 contract: a check that gates anything ships with a fixture where
# the guarded condition is VIOLATED and the check is shown to FAIL. Here the
# violated condition is the #3608 incident itself — a test context driving
# `session-start` (which registers into the sessions registry) without
# CHORUS_SESSIONS_DIR. Pre-membrane that write landed in the LIVE registry
# and misrouted nudges team-wide. Post-membrane it must REFUSE, typed.
#
# Every spawn below redirects HOME into $BATS_TEST_TMPDIR, so even the
# membrane's own violation emit (which goes to $HOME/.chorus/chorus.log by
# design — AC5) lands in the test's world, never the live spine.

SHIM="$CHORUS_ROOT/platform/services/chorus-hooks/target/debug/chorus-hook-shim"

setup() {
  [ -x "$SHIM" ] || skip "shim not built (cargo build in chorus-hooks first)"
  export TEST_HOME="$BATS_TEST_TMPDIR/home"
  mkdir -p "$TEST_HOME/.chorus"
}

# --- AC4 negative proof: the #3608 poisoning shape REFUSES post-fix ---

@test "membrane refuses session-start without CHORUS_SESSIONS_DIR (the #3608 repro)" {
  # BATS_TEST_TMPDIR is exported by bats itself → the shim classifies this a
  # test context. Spine seam provided, sessions seam deliberately ABSENT.
  run env HOME="$TEST_HOME" \
      CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log" \
      CHORUS_SESSIONS_DIR= \
      "$SHIM" session-start silas
  echo "status: $status"
  echo "output: $output"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "MEMBRANE REFUSED"
  echo "$output" | grep -q "sessions-registry"
  echo "$output" | grep -q "CHORUS_SESSIONS_DIR"
  # And nothing was registered anywhere under the test HOME.
  [ ! -d "$TEST_HOME/.chorus/sessions" ] || [ -z "$(ls -A "$TEST_HOME/.chorus/sessions")" ]
}

@test "membrane refuses spine write without CHORUS_LOG_FILE (the #2491 repro)" {
  run env HOME="$TEST_HOME" \
      CHORUS_LOG_FILE= \
      CHORUS_SESSIONS_DIR="$BATS_TEST_TMPDIR/sessions" \
      "$SHIM" log test.membrane.probe silas
  echo "status: $status"
  echo "output: $output"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "MEMBRANE REFUSED"
  echo "$output" | grep -q "'spine'"
  echo "$output" | grep -q "CHORUS_LOG_FILE"
}

# --- AC5: a violation is visible on the spine, never silent ---

@test "membrane violation emits membrane.violation to the (redirected) spine" {
  run env HOME="$TEST_HOME" \
      CHORUS_LOG_FILE= \
      CHORUS_SESSIONS_DIR="$BATS_TEST_TMPDIR/sessions" \
      "$SHIM" log test.membrane.probe silas
  [ "$status" -ne 0 ]
  [ -f "$TEST_HOME/.chorus/chorus.log" ]
  grep -q '"event":"membrane.violation"' "$TEST_HOME/.chorus/chorus.log"
  grep -q '"surface":"spine"' "$TEST_HOME/.chorus/chorus.log"
}

# --- The permitted crossings: overrides and prod context still work ---

@test "with both seams set, session-start succeeds and registers in the test world only" {
  run env HOME="$TEST_HOME" \
      CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log" \
      CHORUS_SESSIONS_DIR="$BATS_TEST_TMPDIR/sessions" \
      "$SHIM" session-start silas
  echo "status: $status"
  echo "output: $output"
  [ "$status" -eq 0 ]
}

@test "explicit CHORUS_CONTEXT=prod is the escape hatch for prod-adjacent runners" {
  # Same test-marker env, but the runner declares itself prod — the membrane
  # honors the explicit contract and resolves the (redirected-HOME) prod path.
  run env HOME="$TEST_HOME" \
      CHORUS_LOG_FILE= \
      CHORUS_SESSIONS_DIR= \
      CHORUS_CONTEXT=prod \
      "$SHIM" log test.membrane.probe silas
  echo "status: $status"
  echo "output: $output"
  [ "$status" -eq 0 ]
  grep -q "test.membrane.probe" "$TEST_HOME/.chorus/chorus.log"
}
