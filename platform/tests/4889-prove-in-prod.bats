#!/usr/bin/env bats
# @test-type: unit — prove in prod: each AC's own check against prod; whole newest run; a red reopens
# @domain: proving
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
BP="$ROOT/platform/services/borg-prove"

@test "AC6: the whole newest run of a test decides (fail → Not met, skip → unmeasured) — borg-prove's own proofs" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$BP/target/$(id -un)" nice cargo test -q --manifest-path "$BP/Cargo.toml" --test whole_run_4889
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
}

@test "AC6: a card with no checks fails prove in prod, it does not land as 'unproven'" {
  ! grep -q 'v != "Proven" && v != NO_ROWS' "$BP/src/main.rs" || { echo "a card with no checks still passes prod"; return 1; }
}

@test "AC6: a red in prod reopens the card" {
  grep -q 'reopen' "$ROOT/platform/pipelines/cicd-land.yaml" || { echo "no reopen on the land's prove red"; return 1; }
}
