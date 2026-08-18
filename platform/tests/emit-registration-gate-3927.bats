#!/usr/bin/env bats
# @test-type: integration
# #3927 — the spine-emit conformance check must BLOCK, not merely notify.
#
# The check was correct for months and stopped nothing: it ran only in
# chorus-health, 20 minutes after the land. Twice in one afternoon (#3917's
# test.script.uncovered, #3926's merge.stale_flag) an unregistered emit landed
# and the alarm fired afterward. These tests exist to prove the check can now
# refuse a violation, because a gate nobody has watched fail is not a gate.
load test_helper

SCRIPT="${CHORUS_ROOT}/platform/scripts/test-werk-emit-conformance.sh"
HOOK="${CHORUS_ROOT}/platform/hooks/pre-commit"

@test "the conformance check passes on the current tree" {
  run env CHORUS_ROOT="$CHORUS_ROOT" bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE: an UNREGISTERED emit makes the check FAIL" {
  # Build a world where a werk source emits an event absent from the schema.
  # The script derives its root from its OWN location, so the fixture must place
  # it at the same relative path a real repo would.
  W="$BATS_TEST_TMPDIR/repo"
  mkdir -p "$W/platform/services/werk-fake/src" "$W/platform/scripts" "$W/designing/schemas"
  cp "${CHORUS_ROOT}/designing/schemas/spine-events.json" "$W/designing/schemas/"
  cp "$SCRIPT" "$W/platform/scripts/"
  cat > "$W/platform/services/werk-fake/src/main.rs" <<'RS'
fn main() { emit_spine("merge.totally_unregistered_3927", &role, &card, &trace, &[]); }
RS
  run env CHORUS_ROOT="$W" bash "$W/platform/scripts/test-werk-emit-conformance.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"merge.totally_unregistered_3927"* ]]
}

@test "NEGATIVE: the pre-commit hook refuses when the check fails" {
  # The hook must exit non-zero on a phantom — not log and continue. Asserted
  # against the hook's own text so a future refactor that drops the exit is caught.
  run grep -A20 '#3927 — spine-emit registration gate' "$HOOK"
  [ "$status" -eq 0 ]
  [[ "$output" == *"exit 1"* ]]
}

@test "the gate is wired into pre-commit, not only chorus-health" {
  run grep -c "test-werk-emit-conformance" "$HOOK"
  [ "$status" -eq 0 ]
  [ "$output" -ge 1 ]
}
