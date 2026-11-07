#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-4916-rows against a stand-in door; no prod
# @domain: alerts
# @card: #4916 · owner: borg-queen
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
S="$ROOT/platform/scripts/land-4916-rows"

setup() {
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D/alerts"
  echo '{"data":[{"name":"ci-main-red"}]}' > "$D/alerts/alerts"
  PORT=$(( 20000 + RANDOM % 20000 ))
  (cd "$D" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) &
  SRV=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s -o /dev/null "http://127.0.0.1:$PORT/alerts/alerts" && break; sleep 0.2; done
  printf '#!/bin/sh\necho "$*" >> "%s/calls"\necho "call → 200"\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/runner"; chmod +x "$BATS_TEST_TMPDIR/runner"
}
teardown() { kill "$SRV" 2>/dev/null || true; }

@test "the dry run plans DELETE for ci-main-red and writes nothing" {
  run env OWL_URL="http://127.0.0.1:$PORT" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"DELETE alerts/alerts/ci-main-red"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/calls" ] || { echo "a dry run wrote: $(cat "$BATS_TEST_TMPDIR/calls")"; return 1; }
}

@test "--apply executes athena-delete for ci-main-red" {
  run env OWL_URL="http://127.0.0.1:$PORT" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q 'call athena-delete domain=alerts class=alerts id=ci-main-red' "$BATS_TEST_TMPDIR/calls" || { cat "$BATS_TEST_TMPDIR/calls"; return 1; }
}

@test "negative proof: a door it cannot read is exit 2, never a guess" {
  run env OWL_URL="http://127.0.0.1:9" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/calls" ] || return 1
}
