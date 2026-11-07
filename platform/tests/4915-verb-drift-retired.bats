#!/usr/bin/env bats
# @test-type: unit — verb-binary-drift.test.sh over a planted bin dir and repo; no spine, no live bin
# @domain: builds
# @card: #4915 · owner: kade
# #4889 retired werk-review; canonical kept its untracked target/ dir, so the drift test's
# folder check still matched and the retired verb read as drifted every night. A crate is
# its Cargo.toml now. Negative proof: the same folder WITH a Cargo.toml is still checked.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

drift() {  # <repo> → runs the drift test with one installed verb, werk-gone
  run env VERB_DRIFT_SPINE="$BATS_TEST_TMPDIR/spine" VERB_DRIFT_BIN="$BATS_TEST_TMPDIR/bin" \
    VERB_DRIFT_ARCHIVE="$BATS_TEST_TMPDIR/none" CHORUS_ROOT="$1" bash "$ROOT/platform/tests/verb-binary-drift.test.sh"
}

setup() {
  mkdir -p "$BATS_TEST_TMPDIR/bin" "$BATS_TEST_TMPDIR/repo/platform/services/werk-gone/target"
  printf '#!/bin/sh\n' > "$BATS_TEST_TMPDIR/bin/werk-gone"; chmod +x "$BATS_TEST_TMPDIR/bin/werk-gone"
  echo '{}' > "$BATS_TEST_TMPDIR/spine"
  git -C "$BATS_TEST_TMPDIR/repo" init -q
}

@test "a retired verb (folder left, no Cargo.toml) is not drift" {
  drift "$BATS_TEST_TMPDIR/repo"
  [[ "$output" != *"FAIL: werk-gone"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: the same verb with a Cargo.toml is still checked (and here, unprovenanced, fails)" {
  printf '[package]\nname="werk-gone"\n' > "$BATS_TEST_TMPDIR/repo/platform/services/werk-gone/Cargo.toml"
  drift "$BATS_TEST_TMPDIR/repo"
  [[ "$output" == *"FAIL: werk-gone"* ]] || { echo "$output"; return 1; }
}
