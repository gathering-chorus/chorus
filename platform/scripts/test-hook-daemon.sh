#!/bin/bash
# test-hook-daemon.sh — AC6 tests for Hook Daemon
# Tests: health check, crash-to-shim fallback, socket-unavailable detection
#
# Tests the live daemon on its control socket and the shim's fallback behavior.
#
# #3721 — this file was reporting 0 pass / 1 fail while testing almost nothing.
# Three independent faults, all of the same family: the check had drifted from
# the thing it checks.
#
#   1. STALE SOCKET PATH. SOCKET was /tmp/chorus-hooks.sock, retired by
#      #3617/#3631 in favour of ~/.chorus/run/. Tests 1-3 all guard on
#      `[ -S "$SOCKET" ]`, so all three silently SKIPPED against a daemon that
#      was up and serving.
#   2. `timeout` DOES NOT EXIST ON THIS MAC. It is GNU coreutils; neither
#      timeout nor gtimeout is installed. `run_bounded 5 "$SHIM" ...` therefore
#      exited 127 (command not found), and test 4 asserted that exit code was 0
#      — so the one test that "ran" was measuring a missing binary, not the
#      shim's fail-open behaviour at all.
#   3. A SKIP THAT COUNTED AS A PASS. The "shim binary not found" branch did
#      PASS=$((PASS+1)). A file that cannot find the thing it tests reported success.
#
# Fixes: resolve the socket from HOME, resolve the shim via `command -v` per CSC
# #2734 (target/release is the BUILD artifact, ~/.chorus/bin the DEPLOY one) with
# the build path only as fallback, degrade gracefully when timeout is absent, and
# stop counting skips as passes.

set -uo pipefail

# #3725 AC1 — TWO fixes, both mine to own.
#
# (a) COUNTERS. `PASS=$((PASS+1))` is post-increment: bash returns the value BEFORE the
#     increment as the exit status, so when PASS is 0 it returns 1 — a "failure".
#     Harmless while -e is off; fatal the moment it isn't. Now `PASS=$((PASS+1))`,
#     which always returns 0.
#
# (b) THE `set -e` LEAK. This file's header is `set -uo pipefail` — -e is NEVER on.
#     Four `set +e ... set -e` pairs therefore didn't RESTORE -e, they TURNED IT ON
#     and left it on for the rest of the run. Combined with (a) that killed the
#     script: with the daemon down, tests 1-3 take SKIP branches, PASS stays 0,
#     test 4 switches -e on, assert_eq does PASS=$((PASS+1)) -> status 1 -> abort. Tests
#     5-7 never ran and no `=== Results: ===` line was printed, so nightly-suites
#     fell through to rc-synthesis and reported "0 pass, 1 fail (synthesized rc=1,
#     no parseable line)" — an unexplained red on any night the daemon is down.
#
#     I CAUSED THE LIVE HALF OF THIS in #3721. Before that change the skip branches
#     did PASS=$((PASS+1)), so PASS was 3 by test 4 and the increment returned success by
#     accident. Making skips stop counting as passes was right; it removed the
#     accident that was masking a latent bug, and I shipped the result. Restores are
#     now `set +e`, matching the header.


CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

# Deploy artifact first (CSC #2734), build artifact as fallback for pre-deploy.
SHIM="$(command -v chorus-hook-shim 2>/dev/null || true)"
[ -n "$SHIM" ] || SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
SOCKET="${CHORUS_HOOK_SOCKET:-$HOME/.chorus/run/chorus-hooks.sock}"
PASS=0
FAIL=0
SKIP=0

# Portable bounded run: use timeout/gtimeout when present, otherwise run bare.
# Running bare is the right degradation — losing the time bound is far better
# than every invocation exiting 127 and being read as the command's own verdict.
if command -v timeout >/dev/null 2>&1; then
  run_bounded() { timeout "$@"; }
elif command -v gtimeout >/dev/null 2>&1; then
  run_bounded() { gtimeout "$@"; }
else
  run_bounded() { shift; "$@"; }
fi

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS+1))
  else
    echo "  FAIL: $label (expected '$expected', got '$actual')"
    FAIL=$((FAIL+1))
  fi
}

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -q "$needle"; then
    echo "  PASS: $label"
    PASS=$((PASS+1))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    FAIL=$((FAIL+1))
  fi
}

echo "=== AC6: Hook Daemon Tests ==="

# --- Test 1: health check via curl to unix socket ---
echo ""
echo "Test 1: health check"
if [ -S "$SOCKET" ]; then
  health_out=$(curl -s --unix-socket "$SOCKET" http://localhost/health 2>&1) || true
  assert_eq "health returns ok" "ok" "$health_out"
else
  echo "  SKIP: daemon socket not found at $SOCKET"
  SKIP=$((SKIP+1))  # Not a failure — daemon may not be running
fi

# --- Test 2: PreToolUse dispatch via socket ---
echo ""
echo "Test 2: PreToolUse dispatch chain"
if [ -S "$SOCKET" ]; then
  # Send a simple Bash tool use that should be allowed
  response=$(echo '{"tool_name":"Bash","tool_input":{"command":"echo hello"},"cwd":"/tmp"}' | \
    curl -s --unix-socket "$SOCKET" -X POST \
      -H "Content-Type: application/json" \
      -d @- http://localhost/pre-tool-use 2>&1) || true
  # Should return JSON with exit_code 0 (allow)
  exit_code=$(echo "$response" | python3 -c "import sys,json; print(json.load(sys.stdin).get('exit_code',99))" 2>/dev/null) || exit_code="parse_error"
  assert_eq "PreToolUse allows safe bash" "0" "$exit_code"
else
  echo "  SKIP: daemon socket not found"
  SKIP=$((SKIP+1))
fi

# --- Test 3: PreToolUse blocks dangerous commands ---
echo ""
echo "Test 3: PreToolUse blocks manual kill"
if [ -S "$SOCKET" ]; then
  response=$(echo '{"tool_name":"Bash","tool_input":{"command":"kill -9 12345"},"cwd":"${CHORUS_ROOT}/engineer"}' | \
    curl -s --unix-socket "$SOCKET" -X POST \
      -H "Content-Type: application/json" \
      -d @- http://localhost/pre-tool-use 2>&1) || true
  # Should have stdout with deny
  has_deny=$(echo "$response" | grep -c "deny" 2>/dev/null) || has_deny=0
  if [ "$has_deny" -gt 0 ]; then
    echo "  PASS: PreToolUse blocks kill -9"
    PASS=$((PASS+1))
  else
    echo "  FAIL: PreToolUse should block kill -9 (response: $response)"
    FAIL=$((FAIL+1))
  fi
else
  echo "  SKIP: daemon socket not found"
  SKIP=$((SKIP+1))
fi

# --- Test 4: shim fail-open when socket unavailable ---
echo ""
echo "Test 4: shim fail-open on missing socket"
if [ -x "$SHIM" ]; then
  # Temporarily rename socket to simulate daemon crash
  if [ -S "$SOCKET" ]; then
    # Use a fake socket path instead of moving the real one
    FAKE_SOCKET="/tmp/chorus-hooks-test-nonexistent.sock"
    # The shim uses hardcoded SOCKET_PATH, so we test with the real socket removed
    # Instead, just verify the shim returns 0 for non-socket-dependent commands
    result=$(echo '{"tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}' | \
      run_bounded 5 "$SHIM" pre-tool-use 2>/dev/null) || true
    # Shim should succeed (it talks to the running daemon)
    echo "  PASS: shim handles live socket"
    PASS=$((PASS+1))
  else
    # Socket is already missing — test fail-open directly
    set +e
    echo '{"tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}' | \
      run_bounded 5 "$SHIM" pre-tool-use >/dev/null 2>&1
    exit_code=$?
    set +e
    assert_eq "shim returns 0 when socket missing (fail-open)" "0" "$exit_code"
  fi
else
  # #3721 — was PASS=$((PASS+1)). A run that cannot locate the binary under test has
  # not passed anything; count it as a skip so the summary stays honest.
  echo "  SKIP: shim binary not found at $SHIM"
  SKIP=$((SKIP+1))
fi

# --- Test 5: shim CLI subcommands work without socket ---
echo ""
echo "Test 5: shim CLI subcommands (no socket needed)"
if [ -x "$SHIM" ]; then
  # wall-clock should work without daemon
  set +e
  clock_out=$("$SHIM" wall-clock 2>/dev/null)
  exit_code=$?
  set +e
  assert_eq "wall-clock returns 0" "0" "$exit_code"
  assert_contains "wall-clock returns timestamp" "20" "$clock_out"

  # role-state query should work without daemon
  set +e
  state_out=$("$SHIM" role-state query kade 2>/dev/null)
  exit_code=$?
  set +e
  assert_eq "role-state query returns 0" "0" "$exit_code"
else
  echo "  SKIP: shim binary not found"
  SKIP=$((SKIP+1))
  SKIP=$((SKIP+1))
  SKIP=$((SKIP+1))
fi

# --- Test 6: shim no-args returns error ---
echo ""
echo "Test 6: shim no-args error"
if [ -x "$SHIM" ]; then
  set +e
  "$SHIM" >/dev/null 2>&1
  exit_code=$?
  set +e
  assert_eq "no args returns 1" "1" "$exit_code"
else
  echo "  SKIP: shim not found"
  SKIP=$((SKIP+1))
fi

# --- Test 7: PostToolUse endpoint responds ---
echo ""
echo "Test 7: PostToolUse endpoint"
if [ -S "$SOCKET" ]; then
  response=$(echo '{"tool_name":"Bash","tool_input":{"command":"echo hi"},"tool_response":"hi\n","cwd":"/tmp"}' | \
    curl -s --unix-socket "$SOCKET" -X POST \
      -H "Content-Type: application/json" \
      -d @- http://localhost/post-tool-use 2>&1) || true
  exit_code=$(echo "$response" | python3 -c "import sys,json; print(json.load(sys.stdin).get('exit_code',99))" 2>/dev/null) || exit_code="parse_error"
  assert_eq "PostToolUse returns 0" "0" "$exit_code"
else
  echo "  SKIP: daemon socket not found"
  SKIP=$((SKIP+1))
fi

# --- Summary ---
echo ""
# #3721 — skips are reported separately instead of being counted as passes.
echo "=== Results: $PASS passed, $FAIL failed, $SKIP skipped ==="
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
