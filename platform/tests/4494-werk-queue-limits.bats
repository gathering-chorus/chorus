#!/usr/bin/env bats
# @test-type: fitness — reads platform/config/dagu.yaml, no live service
# @domain: pipelines — how many werk runs go at once (#4494)
# Jeff, 2026-10-11 09:06: "hard wip of 2 werk cards running and 1 werk land".
# The box measured load 28 at 09:02 with three card runs compiling at once.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CFG="$ROOT/platform/config/dagu.yaml"
}

# the max_concurrency of one named queue in a dagu config
limit() {
  awk -v q="$1" '$0 ~ "- name: "q"$" {f=1; next} f && /max_concurrency:/ {print $2; exit}' "$2"
}

@test "two card runs at once" {
  [ "$(limit werk "$CFG")" = 2 ]
}

@test "one land at a time" {
  [ "$(limit werk-land "$CFG")" = 1 ]
}

@test "NEGATIVE PROOF: a config allowing three card runs fails the check" {
  sed 's/max_concurrency: 2/max_concurrency: 3/' "$CFG" > "$BATS_TEST_TMPDIR/dagu.yaml"
  [ "$(limit werk "$BATS_TEST_TMPDIR/dagu.yaml")" = 3 ]
  run test "$(limit werk "$BATS_TEST_TMPDIR/dagu.yaml")" = 2
  [ "$status" -ne 0 ]
}
