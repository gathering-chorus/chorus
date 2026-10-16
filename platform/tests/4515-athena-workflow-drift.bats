#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — chorus-athena's workflows are generated from the graph (#4515)
# What Jeff sees: platform/pipelines/athena.yaml (Model → Make → Deploy staging →
# Validate staging, no go; on success it starts the card's werk run) and
# athena-land.yaml (the prod Land a werk land starts) are exactly what
# chorus-make writes from their rows, and a hand edit is caught before it lands.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  FIX="$ROOT/platform/services/chorus-make/tests/fixtures"
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

# the lines of a workflow's handler_on block from `<name>:` up to the next handler or steps
handler() { awk -v h="  $1:" '$0==h {f=1; next} f && (/^  [a-z]+:$/ || /^steps:/) {exit} f' "$2"; }

@test "both athena workflows match what chorus-make generates" {
  run "$CM" check athena "$FIX/athena-rows.tsv" "$ROOT/platform/pipelines/athena.yaml"
  [ "$status" -eq 0 ]
  run "$CM" check athena-land "$FIX/athena-land-rows.tsv" "$ROOT/platform/pipelines/athena-land.yaml"
  [ "$status" -eq 0 ]
}

@test "athena has no go and no land, and validates staging" {
  ! grep -qE 'human\.task|dag\.enqueue|--landed' "$ROOT/platform/pipelines/athena.yaml" || return 1
  grep -q 'call athena-validate args=--staging"' "$ROOT/platform/pipelines/athena.yaml" || return 1
}

@test "a passing athena run starts the card's werk run; a failing one only nudges" {
  handler success "$ROOT/platform/pipelines/athena.yaml" | grep -q 'platform/scripts/werk-v2 ${CARD} ${ROLE} ||' || return 1
  # NEGATIVE PROOF: the failure handler enqueues nothing
  run handler failure "$ROOT/platform/pipelines/athena.yaml"
  [[ "$output" == *"ops-nudge"* ]] || return 1
  [[ "$output" != *"werk-v2"* && "$output" != *"enqueue"* ]] || return 1
}

@test "the land writes prod from the landed commit, on its own queue, and never enqueues itself" {
  grep -q 'call athena-deploy args=--card args=${CARD} args=--landed args=--target args=canonical"' "$ROOT/platform/pipelines/athena-land.yaml" || return 1
  grep -qx 'queue: athena-land' "$ROOT/platform/pipelines/athena-land.yaml" || return 1
  [ -z "$(handler success "$ROOT/platform/pipelines/athena-land.yaml")" ] || return 1
  # dagu's config names the queue, one model land at a time
  awk '$0 ~ "- name: athena-land$" {f=1; next} f && /max_concurrency:/ {print $2; exit}' \
    "$ROOT/platform/config/dagu.yaml" | grep -qx 1 || return 1
}

@test "negative proof: a hand-edited athena workflow is drift" {
  sed 's/werk-v2 ${CARD}/werk-v2-skip ${CARD}/' "$ROOT/platform/pipelines/athena.yaml" > "$BATS_TEST_TMPDIR/edited.yaml"
  run "$CM" check athena "$FIX/athena-rows.tsv" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
  [[ "$output" == *"drift at line"* ]] || return 1
  sed 's/args=--landed //' "$ROOT/platform/pipelines/athena-land.yaml" > "$BATS_TEST_TMPDIR/edited-land.yaml"
  run "$CM" check athena-land "$FIX/athena-land-rows.tsv" "$BATS_TEST_TMPDIR/edited-land.yaml"
  [ "$status" -eq 1 ]
}

@test "negative proof: the handler check catches a werk start moved to failure" {
  # the check above must be able to go red: a workflow whose failure handler
  # starts werk is caught by the same lines
  # #4562 — the failure line now carries the /cws why, so match its stable prefix
  sed 's#ops-nudge ${ROLE} "werk v2 \#${CARD} #werk-v2 ${CARD} ${ROLE}; ops-nudge ${ROLE} "x #' \
    "$ROOT/platform/pipelines/athena.yaml" > "$BATS_TEST_TMPDIR/bad.yaml"
  ! cmp -s "$ROOT/platform/pipelines/athena.yaml" "$BATS_TEST_TMPDIR/bad.yaml" || return 1
  run handler failure "$BATS_TEST_TMPDIR/bad.yaml"
  [[ "$output" == *"werk-v2"* ]] || return 1
}

@test "dagu itself loads both athena workflows" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  run dagu validate "$ROOT/platform/pipelines/athena.yaml"
  [ "$status" -eq 0 ]
  run dagu validate "$ROOT/platform/pipelines/athena-land.yaml"
  [ "$status" -eq 0 ]
}
