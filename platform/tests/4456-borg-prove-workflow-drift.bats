#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — borg-prove's workflows are generated from the graph (#4456)
# What Jeff sees: platform/pipelines/borg-prove.yaml and borg-prove-land.yaml are
# exactly what chorus-make writes from borg-prove's rows; a failure routes the
# step to its onFail; a hand edit is caught before it lands.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  ROWS="$ROOT/platform/services/chorus-make/tests/fixtures/borg-prove-rows.tsv"
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

@test "both borg-prove workflows match what chorus-make generates" {
  run "$CM" check borg-prove "$ROWS" "$ROOT/platform/pipelines/borg-prove.yaml"
  [ "$status" -eq 0 ]
  run "$CM" check borg-prove-land "$ROWS" "$ROOT/platform/pipelines/borg-prove-land.yaml"
  [ "$status" -eq 0 ]
}

@test "a failure routes through borg-prove with each step's onFail" {
  grep -q 'borg-prove route ${CARD} ${ROLE} ${DAG_RUN_ID} skill-borg-intake=value-stream-step-designing' "$ROOT/platform/pipelines/borg-prove.yaml" || return 1
  grep -q 'skill-borg-verify-in-prod=value-stream-step-borg-surface' "$ROOT/platform/pipelines/borg-prove-land.yaml" || return 1
}

@test "negative proof: a hand-edited borg-prove workflow is drift" {
  sed 's/value-stream-step-build/value-stream-step-shipping/' "$ROOT/platform/pipelines/borg-prove.yaml" > "$BATS_TEST_TMPDIR/edited.yaml"
  run "$CM" check borg-prove "$ROWS" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
}

@test "dagu itself loads both borg-prove workflows" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  run dagu validate "$ROOT/platform/pipelines/borg-prove.yaml"
  [ "$status" -eq 0 ]
  run dagu validate "$ROOT/platform/pipelines/borg-prove-land.yaml"
  [ "$status" -eq 0 ]
}
