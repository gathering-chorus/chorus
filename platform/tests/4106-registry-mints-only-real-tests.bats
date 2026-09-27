#!/usr/bin/env bats
# #4185 — repointed again: the parsers moved from the Python library (#4159)
# into the crawler crate (chorus-crawl, src/cases.rs). Same behaviour, same
# asserts, third home — the seams are the binary's own (--names-of, --covers-of,
# --check-shares, --classify), no store, no network.
# @test-type: unit — hermetic. Uses the tagger's --names-of seam (#4022): one
# @domain: tests — the product domain this suite guards (#4334)
# file in, the case names the registry WOULD hold out, no store, no network.
#
# #4106 — a registered test must be a test that can actually run. Three ways
# the registry minted names nothing could ever match, found by classifying all
# 169 never-run entries on 2026-09-04:
#   90  the file's own basename, invented whenever no case extractor exists
#       for the kind (.sh, .feature, .py). No runner ever emits a case called
#       "daemon-env-3197.test.sh", so each one is a permanent never-ran row.
#    4  a template literal captured raw — "…ephemeral port ${TEST_PORT}" — the
#       runner emits the interpolated value, so the two never join.
#    2  a `regex.test('some string')` call scraped as a test declaration: the
#       pattern matched `test(` after a dot. One of them registered
#       "<button>Log in</button>" as a test.
# Negative proofs (#3734): each violating fixture is shown to mint nothing,
# and the controls show real names are still registered.

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"; [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"; [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  TMP="$BATS_TEST_TMPDIR"
}

names_of() { "$BIN" --names-of "$1"; }

@test "negative proof: a regex .test('...') call is not a test declaration" {
  f="$TMP/dotcall.test.ts"
  printf '%s\n' \
    "describe('vocab', () => {" \
    "  test('no page ships the words Log in', () => {" \
    "    expect(/Log in/.test('<button>Log in</button>')).toBe(true);" \
    "    expect(/Log in/.test('handleAuthLogin login')).toBe(false);" \
    "  });" \
    "});" > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  has "no page ships the words Log in"
  lacks "<button>"
  lacks "handleAuthLogin"
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}

@test "negative proof: a template-literal name is not registered — it can never match at runtime" {
  f="$TMP/tmpl.test.ts"
  printf '%s\n' \
    'it(`Clearing is running on ephemeral port ${TEST_PORT}`, () => {});' \
    "it('a plain name that does match', () => {});" > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  has "a plain name that does match"
  lacks '${TEST_PORT}'
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}

@test "a shell suite is registered at file grain, by its basename" {
  # #4106 second commit: shell suites got a lane, and the runner stores ONE
  # verdict per suite via shell_suite_case, which emits exactly the basename.
  # So the registry must hold that same string or the row can never join.
  # This case previously asserted the opposite — written before the lane
  # existed, never re-run after it did.
  f="$TMP/daemon-env.test.sh"
  printf '%s\n' '#!/bin/sh' 'echo checking' 'exit 0' > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [ "$output" = "daemon-env.test.sh" ]
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}

@test "negative proof: a kind with no case extractor and no lane mints nothing" {
  # The original proof, moved to a kind that genuinely has neither: nothing
  # runs a bare .py as a suite, so inventing a name for it would recreate the
  # permanent never-ran row this card exists to kill.
  f="$TMP/helper.py"
  printf '%s\n' 'def helper():' '    return 1' > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "control: real jest names are still registered whole" {
  f="$TMP/real.test.ts"
  printf '%s\n' \
    "it('eventFrame is NIP-01 [\"EVENT\", event]', () => {});" \
    "it(\"a double-quoted name with 'inner' quotes\", () => {});" > "$f"
  run names_of "$f"
  has 'eventFrame is NIP-01 ["EVENT", event]'
  has "a double-quoted name with 'inner' quotes"
}

@test "control: bats @test names are still registered, escapes and all" {
  f="$TMP/guard.bats"
  printf '%s\n' '@test "no file hardcodes /Users/<name>/ (use \$CHORUS_ROOT)" {' '  true' '}' > "$f"
  run names_of "$f"
  has "no file hardcodes /Users/<name>/"
}

@test "control: a rust test fn is still registered" {
  f="$TMP/units.rs"
  printf '%s\n' '#[test]' 'fn walks_the_ledger() { }' > "$f"
  run names_of "$f"
  has "walks_the_ledger"
}

# The other half of dropping the invented name: the files must not become
# invisible. The crawler names them on its report line (no_case_report, a unit
# test in src/cases.rs); here the seam shows the file is registered with NO case
# rather than silently skipped or given an invented one.
@test "no-case files yield nothing at the seam, never an invented basename" {
  f="$TMP/helper_test.py"
  printf '%s\n' "def test_helper():" "    assert 1" > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "a playwright spec is registered at file grain — the ui lane's identity (#4045), not its inner titles" {
  f="$TMP/flow.spec.cjs"
  printf '%s\n' "test('a playwright flow', async () => {});" > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [ "$output" = "flow.spec.cjs" ]
}

@test "negative proof: an @test written inside a string fixture is not a test declaration" {
  f="$TMP/fixture-builder.bats"
  printf '%s\n' \
    '@test "the real case" {' \
    "  printf '@test \"a fixture case\" {\\n  true\\n}\\n' > \"\$BATS_TEST_TMPDIR/x.bats\"" \
    '}' > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  has "the real case"
  lacks "a fixture case"
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}
