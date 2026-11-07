#!/usr/bin/env bats
# @test-type: fitness — reads platform/config/dagu.yaml, no live service
# @domain: pipelines — how many werk runs go at once (#4494)
# Jeff, 2026-10-11 09:06: "hard wip of 2 werk cards running and 1 werk land".
# The box measured load 28 at 09:02 with three card runs compiling at once.
# #4675 — Jeff, 2026-10-21 11:05: one card run at a time ("tortoise and hare");
# load was 17 with one test run, 40 with two.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CFG="$ROOT/platform/config/dagu.yaml"
}

# the max_concurrency of one named queue in a dagu config
limit() {
  awk -v q="$1" '$0 ~ "- name: "q"$" {f=1; next} f && /max_concurrency:/ {print $2; exit}' "$2"
}

@test "one card run at a time (#4675)" {
  [ "$(limit werk "$CFG")" = 1 ]
}

@test "one land at a time" {
  [ "$(limit werk-land "$CFG")" = 1 ]
}

@test "NEGATIVE PROOF: a config allowing two card runs fails the check" {
  awk '/- name: werk$/ {f=1} f && /max_concurrency:/ {sub(/[0-9]+/, "2"); f=0} {print}' "$CFG" > "$BATS_TEST_TMPDIR/dagu.yaml"
  [ "$(limit werk "$BATS_TEST_TMPDIR/dagu.yaml")" = 2 ]
  run test "$(limit werk "$BATS_TEST_TMPDIR/dagu.yaml")" = 1
  [ "$status" -ne 0 ]
}

@test "clearing queue concurrency matches team active chunk limit (#4682)" {
  [ "$(limit clearing "$CFG")" = 3 ]
}

@test "NEGATIVE PROOF: a config allowing four clearing runs fails the check (#4682)" {
  awk '/- name: clearing$/ {f=1} f && /max_concurrency:/ {sub(/[0-9]+/, "4"); f=0} {print}' "$CFG" > "$BATS_TEST_TMPDIR/dagu-clearing.yaml"
  [ "$(limit clearing "$BATS_TEST_TMPDIR/dagu-clearing.yaml")" = 4 ]
  run test "$(limit clearing "$BATS_TEST_TMPDIR/dagu-clearing.yaml")" = 3
  [ "$status" -ne 0 ]
}
