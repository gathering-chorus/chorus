#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — werk v2's workflow is generated from the graph (#4465)
# What Jeff sees: platform/pipelines/cicd.yaml (dagu's dags_dir) is exactly what
# chorus-make writes from the cicd rows; a hand edit is caught before it lands.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  WF="$ROOT/platform/pipelines/cicd.yaml"
  ROWS="$ROOT/platform/services/chorus-make/tests/fixtures/cicd-rows.tsv"
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

@test "the committed workflow matches what chorus-make generates" {
  [ -f "$WF" ]
  run "$CM" check cicd "$ROWS" "$WF"
  [ "$status" -eq 0 ]
}

@test "negative proof: a hand-edited workflow is drift" {
  sed 's/werk-test/werk-test --skip-all/' "$WF" > "$BATS_TEST_TMPDIR/edited.yaml"
  run "$CM" check cicd "$ROWS" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
}

@test "a missing workflow fails, never passes" {
  run "$CM" check cicd "$ROWS" "$BATS_TEST_TMPDIR/absent.yaml"
  [ "$status" -ne 0 ]
}

# #4474: the go step's missing id passed every Rust test and only dagu refused it.
@test "dagu itself loads and dry-runs the committed workflow" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  run env DAGU_HOME="$BATS_TEST_TMPDIR/dagu" dagu dry "$WF"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Result: Succeeded"* ]] || return 1
}

@test "negative proof: dagu refuses a go step with no id" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  grep -v '    id: skill_go' "$WF" > "$BATS_TEST_TMPDIR/noid.yaml"
  run env DAGU_HOME="$BATS_TEST_TMPDIR/dagu" dagu dry "$BATS_TEST_TMPDIR/noid.yaml"
  [ "$status" -ne 0 ]
}
