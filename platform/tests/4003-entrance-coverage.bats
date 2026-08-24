#!/usr/bin/env bats
# @test-type: unit — hermetic: fixture allowlist + fixture ui-pages JSON via a local file URL
# #4003 — the entrance-coverage check. Jeff's experience: a link rendered on his
# /chorus entrance that the governed allowlist does not carry must RED here,
# naming the path — instead of being discovered by clicking a dead tile (the
# #4002 Borg case, invisible in the guard's own log because every refusal reads
# as a plain 404).

CHECK="$BATS_TEST_DIRNAME/../scripts/check-entrance-coverage.sh"

setup() {
  ALLOW="$BATS_TEST_TMPDIR/allow.txt"
  UI="$BATS_TEST_TMPDIR/ui.json"
}

# file:// keeps the fixture hermetic — no port, no server process to leak
run_check() {
  SHARE_ALLOW_FILE="$ALLOW" UI_PAGES_URL="file://$UI" bash "$CHECK"
}

@test "covered: every rendered link carried by a section prefix or exact file → exit 0" {
  printf '/borg\n/loom\n/security.html\n' > "$ALLOW"
  printf '{"claimed":{"borg":[{"href":"/borg/index.html"}]},"misc":[{"href":"/loom/decisions.html"},{"href":"/security.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 0 ]
  [[ "$output" == *"3/3 rendered links covered"* ]]
}

@test "NEGATIVE PROOF: a rendered-but-unlisted link REDS and names the path (#3734)" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"},{"href":"/attention-analytics.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/attention-analytics.html"* ]]
  [[ "$output" == *"UNCOVERED"* ]]
}

@test "NEGATIVE PROOF: prefix matching respects path boundaries — /borgX is not covered by /borg" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borgX/sneaky.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/borgX/sneaky.html"* ]]
}

@test "UNMEASURABLE, never vacuous green: unreachable ui-pages → exit 2" {
  printf '/borg\n' > "$ALLOW"
  run env SHARE_ALLOW_FILE="$ALLOW" UI_PAGES_URL="http://127.0.0.1:9/nope.json" bash "$CHECK"
  [ "$status" -eq 2 ]
  [[ "$output" == *"UNMEASURABLE"* ]]
}

@test "NEGATIVE PROOF: a bare / entry is the ROOT only — it must not score every link covered" {
  # This exact hole made the first version of this check report 77/77 while 33
  # links were genuinely dead: root-as-prefix is how a coverage check goes hollow.
  printf '/\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/"},{"href":"/attention-analytics.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/attention-analytics.html"* ]]
  [[ "$output" == *"1/2 rendered links covered"* ]]
}
