#!/usr/bin/env bats
# @test-type: fitness:security
# @domain: security — the share guard these fixtures drive (#3644)
#
# #4455 — the share guard finds its lib/ from __file__ (#4446). Three fixtures
# exec the guard's source without one, so test-chorus-share.sh went red in the
# nightly. Each fixture now supplies __file__. This runs the suite on a card,
# and proves the __file__ line is what makes the fixtures work.
#
# Covers: platform/tests/fixtures/verify-cookie-vectors.py
# Covers: platform/tests/fixtures/probe-safe-return.py
# Covers: platform/tests/fixtures/probe-discover.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/.."
  GUARD="$ROOT/scripts/chorus-share-guard.py"
  FIX="$ROOT/tests/fixtures"
}

@test "test-chorus-share.sh passes with every fixture" {
  # stdin, stdout and fd 3 off the bats pipes: the suite's stub servers would
  # otherwise hold them open and the test would never return
  bash "$ROOT/scripts/test-chorus-share.sh" </dev/null > "$BATS_TEST_TMPDIR/share.out" 2>&1 3>&- || true
  grep -q "=== Results: [0-9]* passed, 0 failed ===" "$BATS_TEST_TMPDIR/share.out"
}

@test "verify-cookie-vectors agrees with the shared vectors" {
  run python3 "$FIX/verify-cookie-vectors.py" "$GUARD" "$FIX/session-cookie-vectors.json"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: the same fixture without __file__ fails" {
  bare="$BATS_TEST_TMPDIR/verify-cookie-vectors.py"
  sed 's/, "__file__": guard}/}/' "$FIX/verify-cookie-vectors.py" > "$bare"
  # the edit must have happened, or this proof proves nothing
  run grep -q '"__file__"' "$bare"
  [ "$status" -eq 1 ]
  run python3 "$bare" "$GUARD" "$FIX/session-cookie-vectors.json"
  [ "$status" -ne 0 ]
}

# The suite's stub servers outlived it: `( cd dir && python3 -m http.server ) &`
# gives $! the subshell, so `kill $!` left python running. 130 were left on
# the Library, and in the werk pipeline they held the test step open.
servers() { pgrep -f "python3 -m http.server|Python -m http.server" | sort; }

@test "test-chorus-share.sh leaves none of its stub servers running" {
  servers > "$BATS_TEST_TMPDIR/before"
  bash "$ROOT/scripts/test-chorus-share.sh" </dev/null > "$BATS_TEST_TMPDIR/share.out" 2>&1 3>&- || true
  sleep 1
  servers > "$BATS_TEST_TMPDIR/after"
  run comm -13 "$BATS_TEST_TMPDIR/before" "$BATS_TEST_TMPDIR/after"
  [ -z "$output" ]
}

# one server started each way, the suite's own line shape, killed the way
# the suite kills it (no eval: that adds a subshell and $! would miss)
free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
left_running() { sleep 1; kill "$2" 2>/dev/null; sleep 1; pgrep -f "http.server $1" || true; }

with_exec() {
  local port; port=$(free_port)
  (cd "$BATS_TEST_TMPDIR" && exec python3 -m http.server "$port" >/dev/null 2>&1) </dev/null 3>&- &
  left_running "$port" $!
}

without_exec() {
  local port; port=$(free_port)
  (cd "$BATS_TEST_TMPDIR" && python3 -m http.server "$port" >/dev/null 2>&1) </dev/null 3>&- &
  left_running "$port" $!
}

@test "exec in the subshell: kill reaches the server" {
  run with_exec
  [ -z "$output" ]
}

@test "NEGATIVE PROOF: without exec, kill leaves the server running" {
  run without_exec
  [ -n "$output" ]
  kill $output 2>/dev/null
}
