#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — the test child and the nightly are generated from their rows (#4540)
# What Jeff sees: platform/pipelines/cicd-test.yaml and nightly.yaml are exactly
# what chorus-make writes from their rows; a hand edit is caught before it lands,
# and dagu itself loads both.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  F="$ROOT/platform/services/chorus-make/tests/fixtures"
  KNOWN=cicd,cicd-test,nightly,athena
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

@test "cicd-test.yaml and nightly.yaml match what chorus-make generates" {
  run "$CM" check-straight cicd-test "$F/cicd-test-rows.tsv" "$F/cicd-test-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/cicd-test.yaml"
  [ "$status" -eq 0 ]
  run "$CM" check-straight nightly "$F/nightly-rows.tsv" "$F/nightly-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/nightly.yaml"
  [ "$status" -eq 0 ]
}

@test "negative proof: a hand-edited nightly is drift" {
  sed 's/SCOPE=all/SCOPE=diff/' "$ROOT/platform/pipelines/nightly.yaml" > "$BATS_TEST_TMPDIR/edited.yaml"
  ! cmp -s "$ROOT/platform/pipelines/nightly.yaml" "$BATS_TEST_TMPDIR/edited.yaml" || return 1
  run "$CM" check-straight nightly "$F/nightly-rows.tsv" "$F/nightly-pipe.tsv" "$KNOWN" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
}

@test "dagu itself validates both" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  for f in cicd-test nightly; do
    run env DAGU_HOME="$BATS_TEST_TMPDIR/dagu" dagu validate "$ROOT/platform/pipelines/$f.yaml"
    [ "$status" -eq 0 ] || { echo "$f: $output"; return 1; }
  done
}
