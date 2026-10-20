#!/usr/bin/env bats
# @test-type: fitness — generated-file drift, no live service
# @domain: pipelines — the test child and the nightly are generated from their rows (#4540)
# What Jeff sees: platform/pipelines/cicd-test.yaml, nightly.yaml, pair.yaml (#4577), demo.yaml (#4502) and card.yaml (#4654) are exactly
# what chorus-make writes from their rows; a hand edit is caught before it lands,
# and dagu itself loads both.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  F="$ROOT/platform/services/chorus-make/tests/fixtures"
  KNOWN=cicd,cicd-test,nightly,athena,pair,demo,card
  CM="$BATS_TEST_TMPDIR/target/debug/chorus-make"
  CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo build -q \
    --manifest-path "$ROOT/platform/services/chorus-make/Cargo.toml" >/dev/null
}

@test "cicd-test.yaml and nightly.yaml match what chorus-make generates" {
  run "$CM" check-straight cicd-test "$F/cicd-test-rows.tsv" "$F/cicd-test-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/cicd-test.yaml"
  [ "$status" -eq 0 ]
  run "$CM" check-straight nightly "$F/nightly-rows.tsv" "$F/nightly-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/nightly.yaml"
  [ "$status" -eq 0 ]
  # #4577 — the pair pipeline
  run "$CM" check-straight pair "$F/pair-rows.tsv" "$F/pair-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/pair.yaml"
  [ "$status" -eq 0 ]
  # #4502 — the demo, its own pipeline that cicd calls as one step (Jeff 10-12 21:18)
  run "$CM" check-straight demo "$F/demo-rows.tsv" "$F/demo-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/demo.yaml"
  [ "$status" -eq 0 ]
  # #4654 — a card's pull and its coding, ahead of athena or cicd
  run "$CM" check-straight card "$F/card-rows.tsv" "$F/card-pipe.tsv" "$KNOWN" "$ROOT/platform/pipelines/card.yaml"
  [ "$status" -eq 0 ]
  # #4578 — athena builds the card's tools first and releases staging last
  run "$CM" check athena "$F/athena-rows.tsv" "$ROOT/platform/pipelines/athena.yaml"
  [ "$status" -eq 0 ]
}

@test "negative proof: an athena without its tools step is drift (#4578)" {
  grep -v 'werk-deploy-demo' "$ROOT/platform/pipelines/athena.yaml" > "$BATS_TEST_TMPDIR/athena.yaml"
  ! cmp -s "$ROOT/platform/pipelines/athena.yaml" "$BATS_TEST_TMPDIR/athena.yaml" || return 1
  run "$CM" check athena "$F/athena-rows.tsv" "$BATS_TEST_TMPDIR/athena.yaml"
  [ "$status" -eq 1 ]
}

@test "negative proof: a pair whose green a hand edit gave to the driver is drift" {
  sed 's/(${NAVIGATOR} answers)/(${ROLE} answers)/' "$ROOT/platform/pipelines/pair.yaml" > "$BATS_TEST_TMPDIR/pair.yaml"
  ! cmp -s "$ROOT/platform/pipelines/pair.yaml" "$BATS_TEST_TMPDIR/pair.yaml" || return 1
  run "$CM" check-straight pair "$F/pair-rows.tsv" "$F/pair-pipe.tsv" "$KNOWN" "$BATS_TEST_TMPDIR/pair.yaml"
  [ "$status" -eq 1 ]
}

@test "negative proof: a card whose done a hand edit gave to Jeff is drift (#4654)" {
  sed 's/(${ROLE} answers)/(jeff answers)/' "$ROOT/platform/pipelines/card.yaml" > "$BATS_TEST_TMPDIR/card.yaml"
  ! cmp -s "$ROOT/platform/pipelines/card.yaml" "$BATS_TEST_TMPDIR/card.yaml" || return 1
  run "$CM" check-straight card "$F/card-rows.tsv" "$F/card-pipe.tsv" "$KNOWN" "$BATS_TEST_TMPDIR/card.yaml"
  [ "$status" -eq 1 ]
}

@test "negative proof: a demo with a gate dropped by hand is drift (#4502)" {
  grep -v 'skill-werk-demo-gate-ops' "$ROOT/platform/pipelines/demo.yaml" > "$BATS_TEST_TMPDIR/demo.yaml"
  ! cmp -s "$ROOT/platform/pipelines/demo.yaml" "$BATS_TEST_TMPDIR/demo.yaml" || return 1
  run "$CM" check-straight demo "$F/demo-rows.tsv" "$F/demo-pipe.tsv" "$KNOWN" "$BATS_TEST_TMPDIR/demo.yaml"
  [ "$status" -eq 1 ]
}

@test "negative proof: a hand-edited nightly is drift" {
  sed 's/SCOPE=all/SCOPE=diff/' "$ROOT/platform/pipelines/nightly.yaml" > "$BATS_TEST_TMPDIR/edited.yaml"
  ! cmp -s "$ROOT/platform/pipelines/nightly.yaml" "$BATS_TEST_TMPDIR/edited.yaml" || return 1
  run "$CM" check-straight nightly "$F/nightly-rows.tsv" "$F/nightly-pipe.tsv" "$KNOWN" "$BATS_TEST_TMPDIR/edited.yaml"
  [ "$status" -eq 1 ]
}

@test "dagu itself validates both" {
  command -v dagu >/dev/null || { echo "dagu not installed: unmeasured, not green"; false; }
  for f in cicd-test nightly pair demo cicd card; do
    run env DAGU_HOME="$BATS_TEST_TMPDIR/dagu" dagu validate "$ROOT/platform/pipelines/$f.yaml"
    [ "$status" -eq 0 ] || { echo "$f: $output"; return 1; }
  done
}
