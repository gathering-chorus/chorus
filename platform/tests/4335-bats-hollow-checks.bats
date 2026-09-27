#!/usr/bin/env bats
# @test-type: unit — runs the lint and bats itself on fixture suites in a temp dir; no live services
# @domain: tests — the product domain this suite guards
# #4335 — on the Macs' bash 3.2, `set -e` ignores a failed `[[ ]]` and a
# `!`-negated command, so bats throws those checks away unless they are the last
# line of the test. bats-hollow-checks finds them and adds `|| return 1`.
#
# Negative proof (#3734): the fixture's false check passes bats before the fix,
# the lint names it, and the same suite goes red after --fix.

LINT="$BATS_TEST_DIRNAME/../scripts/bats-hollow-checks"

# Fixtures are written with printf, one line per argument. bats rewrites any
# line of this file that starts with the test keyword, heredoc bodies included,
# so a heredoc fixture would be mangled before it reached the disk.
T='@test'

setup() {
  FIX="$BATS_TEST_TMPDIR/fixture.bats"
  printf '%s\n' \
    "$T \"false [[ ]] in the middle\" {" \
    '  [[ 1 == 2 ]]' \
    '  true' \
    '}' \
    "$T \"false ! in the middle\" {" \
    '  ! true' \
    '  true' \
    '}' > "$FIX"
}

@test "NEGATIVE PROOF: a false mid-test [[ ]] and ! pass bats before the fix" {
  run bats "$FIX"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok 1 false [[ ]] in the middle"* ]] || return 1
  [[ "$output" == *"ok 2 false ! in the middle"* ]] || return 1
}

@test "the lint names each hollow line by file and line, and exits 1" {
  run python3 "$LINT" "$FIX"
  [ "$status" -eq 1 ]
  [[ "$output" == *"fixture.bats:2: [[ 1 == 2 ]]"* ]] || return 1
  [[ "$output" == *"fixture.bats:6: ! true"* ]] || return 1
}

@test "after --fix the same suite goes red on both tests, and the lint is clean" {
  python3 "$LINT" --fix "$FIX"
  run bats "$FIX"
  [ "$status" -ne 0 ]
  [[ "$output" == *"not ok 1"* ]] || return 1
  [[ "$output" == *"not ok 2"* ]] || return 1
  run python3 "$LINT" "$FIX"
  [ "$status" -eq 0 ]
}

@test "lines the lint must leave alone: [ ], existing ||, &&, heredocs, quoted scripts, top level, comments" {
  CLEAN="$BATS_TEST_TMPDIR/clean.bats"
  printf '%s\n' \
    '[[ -n "$TOP_LEVEL" ]]' \
    "$T \"x\" {" \
    '  [ 1 = 1 ]' \
    '  [[ 1 == 1 ]] || return 1' \
    '  [[ -f a ]] && rm a' \
    "  cat > f <<'INNER'" \
    '  [[ in a heredoc ]]' \
    'INNER' \
    "  run bash -c '" \
    '  [[ in a quoted script ]]' \
    "  '" \
    '  # [[ in a comment ]]' \
    '}' > "$CLEAN"
  before=$(cat "$CLEAN")
  run python3 "$LINT" "$CLEAN"
  [ "$status" -eq 0 ]
  python3 "$LINT" --fix "$CLEAN"
  [ "$(cat "$CLEAN")" = "$before" ]
}

@test "a trailing comment stays a comment after --fix" {
  C="$BATS_TEST_TMPDIR/comment.bats"
  printf '%s\n' "$T \"c\" {" '  [[ 1 == 1 ]]  # why' '  true' '}' > "$C"
  python3 "$LINT" --fix "$C"
  run sed -n 2p "$C"
  [ "$output" = '  [[ 1 == 1 ]] || return 1  # why' ]
}

@test "every bats suite in the repo is clean" {
  run python3 "$LINT" "$BATS_TEST_DIRNAME/../.."
  echo "$output" | tail -20
  [ "$status" -eq 0 ]
}
