#!/usr/bin/env bats
# @test-type: unit — hermetic: fixture files in $BATS_TEST_TMPDIR, the tagger's
# --names-of / --check-shares seams, no store.
#
# #4022 — 594 results per nightly had no registered identity and the census
# said 887 tests "never ran". They ran; the registry held their names cut at
# the first quote inside the name. Negative proof: the exact names that were
# truncated are now whole; control: plain names unchanged. And the share gate
# stands down for a one-file corpus (it went red on the tagger's own test).

TAGGER="$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py"

@test "negative proof: a jest name containing quotes is registered WHOLE" {
  f="$BATS_TEST_TMPDIR/relay.test.ts"
  cat > "$f" <<'TS'
describe('#3696 relay framing (pure)', () => {
  it('eventFrame is NIP-01 ["EVENT", event]', () => {});
  it(`has zero = ${n} rows`, () => {});
  it("says 'hi' to it", () => {});
  test.only('plain name', () => {});
});
TS
  run python3 "$TAGGER" --names-of "$f"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = 'eventFrame is NIP-01 ["EVENT", event]' ]
  [ "${lines[1]}" = 'has zero = ${n} rows' ]
  [ "${lines[2]}" = "says 'hi' to it" ]
  [ "${lines[3]}" = 'plain name' ]
  [ "${#lines[@]}" -eq 4 ]
}

@test "control: bats and rust names are untouched" {
  b="$BATS_TEST_TMPDIR/x.bats"; printf '@test "one thing" {\n  true\n}\n' > "$b"
  run python3 "$TAGGER" --names-of "$b"; [ "${lines[0]}" = "one thing" ]
  r="$BATS_TEST_TMPDIR/x.rs"; printf '#[test]\nfn does_x() {}\n' > "$r"
  run python3 "$TAGGER" --names-of "$r"; [ "${lines[0]}" = "does_x" ]
}

@test "share gate stands down below the corpus floor, still fires above it" {
  echo '{"services": 1}' > "$BATS_TEST_TMPDIR/one.json"
  run python3 "$TAGGER" --check-shares "$BATS_TEST_TMPDIR/one.json"
  [ "$status" -eq 0 ]
  echo '{"services": 40, "x": 5}' > "$BATS_TEST_TMPDIR/big.json"
  run python3 "$TAGGER" --check-shares "$BATS_TEST_TMPDIR/big.json"
  [ "$status" -ne 0 ]
  [[ "$output" == *"covers-share gate RED"* ]]
}
