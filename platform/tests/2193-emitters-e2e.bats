#!/usr/bin/env bats
# @test-type: e2e — full-flow end-to-end
load test_helper
# 2193-emitters-e2e — source-shape + smoke assertions for #2193 semantic
# spine emitters that live in bash (commit.landed, test.delta) and the
# derive + coherence-check scripts. (#4028 retired both bash scripts — state is
# derived in platform/api/src/derive-role-state.ts now; #4080 retired their 5 tests.)

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

@test "spine-events.json registers ac.ticked, commit.landed, test.delta" {
  grep -qE '"ac\.ticked"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
  grep -qE '"commit\.landed"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
  grep -qE '"test\.delta"' "$CHORUS_ROOT/designing/schemas/spine-events.json"
}
# (#3205) The gemba-tick noise-filter test was retired with gemba-tick.sh. Noise
# filtering now lives in the observer hook (observer.rs skips Read/Glob/Grep/etc.
# before writing observations.jsonl), and gemba polls the pulse-gather verb — see
# platform/services/pulse-gather/tests/units.rs for the delta-surface coverage.
