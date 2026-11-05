#!/usr/bin/env bats
# @test-type: unit — a review round reads only what changed since the last one, through the ledger
# @domain: pipelines
# @card: #4889 · owner: silas
# What Jeff sees (11-05 10:06, "isnt there just a new commit to review?"): a rerun after a
# fix reads only the new commit. platform/services/werk-demo/src/lib.rs records the commit
# each review read up to (last_reviewed), reviews <that>..HEAD next time, and keeps a
# ledger (apply_ledger): an old finding carries forward, red, until a commit touches its
# file and it is not raised again. There is no waiver (Kade 10:24).
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
WD="$ROOT/platform/services/werk-demo"

@test "the ledger: carried until fixed, fixed when its file changes, new defects caught" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$WD/target/$(id -un)" nice cargo test -q --manifest-path "$WD/Cargo.toml" --test ledger_4889
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"7 passed"* ]] || { echo "$output" | tail -5; return 1; }
}

@test "a real gate round: same commit carries the red, a fixing commit clears it, a new commit's defect is caught" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$WD/target/$(id -un)" nice cargo test -q --manifest-path "$WD/Cargo.toml" --test code_step_4889 --test steps_4502
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
}

@test "the gate diff is the delta range when a round was reviewed before" {
  grep -q 'format!("{c}..HEAD")' "$WD/src/lib.rs" || return 1
  grep -q 'record_gate_at(home, role, card, gate, &result, &findings, round, reviewed)' "$WD/src/lib.rs" || return 1
}
