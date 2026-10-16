#!/usr/bin/env bats
# @test-type: unit — a dry-run plan script, no live service
# @domain: pipelines — the store writes #4540's land needs, shown before Jeff's yes
# What Jeff sees: platform/scripts/land-4540-rows prints the writes and writes
# nothing; when the door cannot be read it says so and guesses no plan.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/land-4540-rows"
}

@test "an unreadable door prints no plan and exits 2" {
  run env OWL_URL=http://127.0.0.1:9 python3 "$SCRIPT"
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"plan: door writes"* ]] || return 1
}

@test "the script only reads: no request carries a write method" {
  run grep -nE "method=|\"(POST|PUT|PATCH|DELETE)\"" "$SCRIPT"
  [ "$status" -eq 1 ]
}

@test "negative proof: a copy that sends a PUT is caught" {
  sed 's/urllib.request.urlopen(f"{DOOR}{path}"/urllib.request.urlopen(urllib.request.Request(f"{DOOR}{path}", method="PUT")/' "$SCRIPT" > "$BATS_TEST_TMPDIR/writes"
  ! cmp -s "$SCRIPT" "$BATS_TEST_TMPDIR/writes" || return 1
  run grep -nE "method=|\"(POST|PUT|PATCH|DELETE)\"" "$BATS_TEST_TMPDIR/writes"
  [ "$status" -eq 0 ]
}
