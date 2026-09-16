#!/usr/bin/env bats
# #4185 — repointed again: the parsers moved from the Python library (#4159)
# into the crawler crate (chorus-crawl, src/cases.rs). Same behaviour, same
# asserts, third home — the seams are the binary's own (--names-of, --covers-of,
# --check-shares, --classify), no store, no network.
# @test-type: unit — hermetic: fixture files in $BATS_TEST_TMPDIR, the tagger's
# --names-of / --check-shares seams, no store.
#
# #4022 — 594 results per nightly had no registered identity and the census
# said 887 tests "never ran". They ran; the registry held their names cut at
# the first quote inside the name. Negative proof: the exact names that were
# truncated are now whole; control: plain names unchanged. And the share gate
# stands down for a one-file corpus (it went red on the tagger's own test).
#
# AMENDED by #4106: the backtick case here was `has zero = ${n} rows`, asserting
# an INTERPOLATED name is registered whole. It is registered whole and can never
# be joined — jest emits the interpolated value, so all four such rows sat in the
# census as never-ran forever. #4106 drops interpolated names at extraction; the
# backtick case kept here is a plain template with no substitution, which is what
# this test was really guarding (the delimiter, not the interpolation).

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"; [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"; [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
}

@test "negative proof: a jest name containing quotes is registered WHOLE" {
  f="$BATS_TEST_TMPDIR/relay.test.ts"
  cat > "$f" <<'TS'
describe('#3696 relay framing (pure)', () => {
  it('eventFrame is NIP-01 ["EVENT", event]', () => {});
  it(`has zero rows`, () => {});
  it("says 'hi' to it", () => {});
  test.only('plain name', () => {});
});
TS
  run "$BIN" --names-of "$f"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = 'eventFrame is NIP-01 ["EVENT", event]' ]
  [ "${lines[1]}" = 'has zero rows' ]
  [ "${lines[2]}" = "says 'hi' to it" ]
  [ "${lines[3]}" = 'plain name' ]
  [ "${#lines[@]}" -eq 4 ]
}

@test "control: bats and rust names are untouched" {
  b="$BATS_TEST_TMPDIR/x.bats"; printf '@test "one thing" {\n  true\n}\n' > "$b"
  run "$BIN" --names-of "$b"; [ "${lines[0]}" = "one thing" ]
  r="$BATS_TEST_TMPDIR/x.rs"; printf '#[test]\nfn does_x() {}\n' > "$r"
  run "$BIN" --names-of "$r"; [ "${lines[0]}" = "does_x" ]
}

@test "share gate stands down below the corpus floor, still fires above it" {
  echo '{"services": 1}' > "$BATS_TEST_TMPDIR/one.json"
  run "$BIN" --check-shares "$BATS_TEST_TMPDIR/one.json"
  [ "$status" -eq 0 ]
  echo '{"services": 40, "x": 5}' > "$BATS_TEST_TMPDIR/big.json"
  run "$BIN" --check-shares "$BATS_TEST_TMPDIR/big.json"
  [ "$status" -ne 0 ]
  has "covers-share gate RED"
}
