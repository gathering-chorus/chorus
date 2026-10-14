#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — chorus-athena's workflows are generated from the graph (#4515)
# What Jeff sees: platform/pipelines/athena.yaml and athena-land.yaml are exactly
# what chorus-make writes from the athena pipeline's rows (Model → Make → Deploy
# → Validate → his go → Land), the land deploys prod from the landed commit and
# says canonical on purpose, and a hand edit is caught before it lands.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  ROWS="$ROOT/platform/services/chorus-make/tests/fixtures/athena-rows.tsv"
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

@test "both athena workflows match what chorus-make generates" {
  run "$CM" check athena "$ROWS" "$ROOT/platform/pipelines/athena.yaml"
  [ "$status" -eq 0 ]
  run "$CM" check athena-land "$ROWS" "$ROOT/platform/pipelines/athena-land.yaml"
  [ "$status" -eq 0 ]
}

@test "the card half stops at Jeff's go and enqueues the land; the land writes prod from the landed commit" {
  grep -q 'action: human.task' "$ROOT/platform/pipelines/athena.yaml" || return 1
  grep -q 'dag: athena-land' "$ROOT/platform/pipelines/athena.yaml" || return 1
  ! grep -q -- '--landed' "$ROOT/platform/pipelines/athena.yaml" || return 1
  grep -q 'call athena-deploy args=--card args=${CARD} args=--landed args=--target args=canonical"' "$ROOT/platform/pipelines/athena-land.yaml" || return 1
}

@test "the model land runs on its own queue and never enqueues itself" {
  grep -qx 'queue: athena-land' "$ROOT/platform/pipelines/athena-land.yaml" || return 1
  ! grep -q 'scripts/athena-land' "$ROOT/platform/pipelines/athena-land.yaml" || return 1
  # dagu's config names the queue, one model land at a time
  awk '$0 ~ "- name: athena-land$" {f=1; next} f && /max_concurrency:/ {print $2; exit}' \
    "$ROOT/platform/config/dagu.yaml" | grep -qx 1 || return 1
}

@test "negative proof: a hand-edited athena workflow is drift" {
  sed 's/args=--landed //' "$ROOT/platform/pipelines/athena-land.yaml" > "$BATS_TEST_TMPDIR/edited.yaml"
  run "$CM" check athena-land "$ROWS" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
  [[ "$output" == *"drift at line"* ]] || return 1
}

@test "dagu itself loads both athena workflows" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  run dagu validate "$ROOT/platform/pipelines/athena.yaml"
  [ "$status" -eq 0 ]
  run dagu validate "$ROOT/platform/pipelines/athena-land.yaml"
  [ "$status" -eq 0 ]
}
