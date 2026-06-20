#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
load test_helper
# 2193-emitters-e2e — source-shape + smoke assertions for #2193 semantic
# spine emitters that live in bash (commit.landed, test.delta) and the
# derive + coherence-check scripts.

CHORUS_ROOT="${CHORUS_ROOT}"

@test "gate-code-tests.sh emits test.delta with passed/failed/delta counts" {
  grep -qE 'test\.delta' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
  grep -qE 'delta_passed=' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
  grep -qE 'delta_failed=' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
  grep -qE 'run_jest_with_delta' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
}

@test "test.delta prior-run file rotates so deltas compute across runs" {
  grep -qE '/tmp/chorus-test-delta-' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
  grep -qE 'emit_test_delta' "$CHORUS_ROOT/platform/scripts/gate-code-tests.sh"
}

@test "derive-role-state script exists, executable, and writes inferred.json" {
  # #2614: writes /tmp/claude-team-scan/kade-inferred.json — same path live
  # daemon reads. Gate behind RUN_INTEGRATION; default cargo/bats run skips.
  [ -z "${RUN_INTEGRATION:-}" ] && skip "axis-4 — writes /tmp/claude-team-scan/kade-inferred.json (set RUN_INTEGRATION=1 to run)"
  [ -x "$CHORUS_ROOT/platform/scripts/derive-role-state" ]
  # Smoke-run for kade; inferred.json should appear and be valid JSON.
  bash "$CHORUS_ROOT/platform/scripts/derive-role-state" kade
  [ -f /tmp/claude-team-scan/kade-inferred.json ]
  python3 -c "import json; d=json.load(open('/tmp/claude-team-scan/kade-inferred.json')); assert d['role']=='kade'; assert d['source']=='inferred'; assert 'ts' in d"
}

@test "derive-role-state rejects unknown role with non-zero exit" {
  run bash "$CHORUS_ROOT/platform/scripts/derive-role-state" jeff
  [ "$status" -ne 0 ]
}

@test "coherence-check script exists and is executable" {
  [ -x "$CHORUS_ROOT/platform/scripts/coherence-check" ]
}

@test "coherence-check has the 60s threshold + fires role.state.drifted on alarm" {
  grep -qE 'THRESHOLD=60' "$CHORUS_ROOT/platform/scripts/coherence-check"
  grep -qE 'role\.state\.drifted' "$CHORUS_ROOT/platform/scripts/coherence-check"
}

@test "coherence-check nudges the drifted role, not jeff" {
  # Source assertion: the nudge target is $ROLE from the loop, not 'jeff' literally.
  run grep -nE '"\$OPS_NUDGE"\s+"jeff"' "$CHORUS_ROOT/platform/scripts/coherence-check"
  [ "$status" -ne 0 ]  # no match = jeff never hard-coded as target
  grep -qE '"\$OPS_NUDGE"\s+"\$ROLE"' "$CHORUS_ROOT/platform/scripts/coherence-check"
}

@test "spine-events.json registers ac.ticked, commit.landed, test.delta" {
  grep -qE '"ac\.ticked"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
  grep -qE '"commit\.landed"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
  grep -qE '"test\.delta"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
}
# (#3205) The gemba-tick noise-filter test was retired with gemba-tick.sh. Noise
# filtering now lives in the observer hook (observer.rs skips Read/Glob/Grep/etc.
# before writing observations.jsonl), and gemba polls the pulse-gather verb — see
# platform/services/pulse-gather/tests/units.rs for the delta-surface coverage.
