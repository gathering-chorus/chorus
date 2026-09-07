#!/usr/bin/env bats
# @test-type: unit — hermetic. Uses the tagger's --names-of seam (#4022): one
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

setup() {
  TAGGER="$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py"
  TMP="$BATS_TEST_TMPDIR"
}

names_of() { python3 "$TAGGER" --names-of "$1"; }

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
  [[ "$output" == *"no page ships the words Log in"* ]]
  [[ "$output" != *"<button>"* ]]
  [[ "$output" != *"handleAuthLogin"* ]]
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}

@test "negative proof: a template-literal name is not registered — it can never match at runtime" {
  f="$TMP/tmpl.test.ts"
  printf '%s\n' \
    'it(`Clearing is running on ephemeral port ${TEST_PORT}`, () => {});' \
    "it('a plain name that does match', () => {});" > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [[ "$output" == *"a plain name that does match"* ]]
  [[ "$output" != *'${TEST_PORT}'* ]]
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
  [[ "$output" == *'eventFrame is NIP-01 ["EVENT", event]'* ]]
  [[ "$output" == *"a double-quoted name with 'inner' quotes"* ]]
}

@test "control: bats @test names are still registered, escapes and all" {
  f="$TMP/guard.bats"
  printf '%s\n' '@test "no file hardcodes /Users/<name>/ (use \$CHORUS_ROOT)" {' '  true' '}' > "$f"
  run names_of "$f"
  [[ "$output" == *"no file hardcodes /Users/<name>/"* ]]
}

@test "control: a rust test fn is still registered" {
  f="$TMP/units.rs"
  printf '%s\n' '#[test]' 'fn walks_the_ledger() { }' > "$f"
  run names_of "$f"
  [[ "$output" == *"walks_the_ledger"* ]]
}

# The other half of dropping the invented name: the files must not become
# invisible. Before #4106 they were counted as tests that never ran; the fix
# must state them, not silence them.
@test "no-case files are reported, never silently dropped" {
  run python3 -c "
import importlib.util,sys
spec=importlib.util.spec_from_file_location('t','$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
print(m.no_case_report(['a/x.sh','b/y.sh','c/z.feature']))
print(m.no_case_report([]))
"
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == *"3 registered file(s) yield no runnable case"* ]]
  [[ "${lines[0]}" == *"sh 2, feature 1"* ]]
  [[ "${lines[1]}" == *"none"* ]]
}

@test "negative proof: an @test written inside a string fixture is not a test declaration" {
  f="$TMP/fixture-builder.bats"
  printf '%s\n' \
    '@test "the real case" {' \
    "  printf '@test \"a fixture case\" {\\n  true\\n}\\n' > \"\$BATS_TEST_TMPDIR/x.bats\"" \
    '}' > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [[ "$output" == *"the real case"* ]]
  [[ "$output" != *"a fixture case"* ]]
  [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ]
}
