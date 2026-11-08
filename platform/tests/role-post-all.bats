#!/usr/bin/env bats
# @test-type: unit — role-post-all against fixture results, ps and principals; no live services
# @domain: identity
#
# #4931 AC3 — one table for every agent principal, role by self-test check, from the
# results each role wrote as itself. Exit 1 if any cell is FAIL, a role never ran its
# self test, or its result is older than its running session.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  ALL="$ROOT/platform/scripts/role-post-all"
  T="$BATS_TEST_TMPDIR"
  export POST_ALL_HOMES="$T/homes" POST_ALL_PS="$T/ps.txt"
  export POST_ALL_PRINCIPALS_CMD="printf '{\"data\":[{\"name\":\"a\",\"principalKind\":\"agent\",\"hostAccount\":\"chorus-a\"},{\"name\":\"b\",\"principalKind\":\"agent\",\"hostAccount\":\"chorus-b\"},{\"name\":\"jeff\",\"principalKind\":\"person\",\"hostAccount\":\"j\"}]}'"
  printf 'chorus-a 00:10 /x/bin/claude -c\nchorus-b 00:10 agy\n' > "$T/ps.txt"
}
result() {  # role acct [fail-check]
  mkdir -p "$T/homes/$2/.chorus/post"
  { echo "at $(date +%s) role $1 account $2 runtime claude"
    for c in identity identity-dir key settings login reach; do
      if [ "$c" = "${3:-}" ]; then echo "FAIL $c: broken"; else echo "ok   $c: fine"; fi
    done; } > "$T/homes/$2/.chorus/post/$1.result"
}

@test "#4931 every agent principal green: one row each, exit 0" {
  result a chorus-a; result b chorus-b
  run "$ALL"
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | grep -cE '^(a|b) ')" -eq 2 ]
  ! printf '%s\n' "$output" | grep -q '^jeff ' || return 1
}

@test "#4931 NEGATIVE PROOF: one FAIL cell makes the table red and names it" {
  result a chorus-a settings; result b chorus-b
  run "$ALL"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "FAIL settings: broken"
}

@test "#4931 NEGATIVE PROOF: a role whose self test never ran is red, never blank-green" {
  result a chorus-a
  run "$ALL"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "b"
  printf '%s\n' "$output" | grep -qF "FAIL no result (role-post never ran as chorus-b)"
}

@test "#4931 NEGATIVE PROOF: a result from before the running session is stale, red" {
  result a chorus-a; result b chorus-b
  sed -i '' "1s/^at [0-9]*/at $(( $(date +%s) - 3600 ))/" "$T/homes/chorus-b/.chorus/post/b.result"
  run "$ALL"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "FAIL older than the running session"
}

@test "#4931 the principals door down is red, never an empty green table" {
  run env POST_ALL_PRINCIPALS_CMD=false "$ALL"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "principals door unreadable"
}

@test "#4931 a result written just before its session started is current, and plain 'claude' is a session" {
  result a chorus-a; result b chorus-b
  sed -i '' "1s/^at [0-9]*/at $(( $(date +%s) - 20 ))/" "$T/homes/chorus-a/.chorus/post/a.result"
  printf 'chorus-a 00:10 claude -c\nchorus-b 00:10 agy\n' > "$T/ps.txt"
  run "$ALL"
  [ "$status" -eq 0 ]
}
