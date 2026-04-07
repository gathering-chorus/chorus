#!/bin/bash
# test-compound-loop.sh — Tests for compound loop reboot survival (#2008)
# AC: after-prompt hooks fire on first prompt of new session, context injected every prompt

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

PASS=0
FAIL=0
HOOKS_LOG="$HOME/Library/Logs/Chorus/chorus-hooks.stdout.log"
HOOKS_STDERR="$HOME/Library/Logs/Chorus/chorus-hooks.stderr.log"

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qi "$needle"; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    ((FAIL++))
  fi
}

echo "=== compound loop reboot survival tests ==="
echo ""

# --- Test 1: Hook server is running ---
echo "Test 1: Hook server is running"
pid=$(launchctl list com.chorus.hooks 2>/dev/null | grep '"PID"' | grep -o '[0-9]*')
if [ -n "$pid" ]; then
  echo "  PASS: hook server running (PID $pid)"
  ((PASS++))
else
  echo "  FAIL: hook server not running"
  ((FAIL++))
fi

# --- Test 2: settings.json has UserPromptSubmit hook ---
echo "Test 2: UserPromptSubmit hook registered in settings.json"
output=$(cat ~/.claude/settings.json 2>/dev/null)
assert_contains "UserPromptSubmit registered" "UserPromptSubmit" "$output"

# --- Test 3: Hook shim binary exists ---
echo "Test 3: Hook shim binary exists"
SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
if [ -x "$SHIM" ]; then
  echo "  PASS: shim binary exists and is executable"
  ((PASS++))
else
  echo "  FAIL: shim binary not found at $SHIM"
  ((FAIL++))
fi

# --- Test 4: Context injection fired in the last 5 minutes ---
echo "Test 4: Context injection fired recently"
recent=$(tail -50 "$HOOKS_LOG" 2>/dev/null | grep 'context-inject.*event.*injected')
if [ -n "$recent" ]; then
  echo "  PASS: context injection fired"
  ((PASS++))
else
  echo "  FAIL: no recent context injection events in hook log"
  ((FAIL++))
fi

# --- Test 5: Multiple roles receiving injection ---
echo "Test 5: Multiple roles receiving context injection"
roles=$(tail -100 "$HOOKS_LOG" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep 'context-inject.*injected' | grep -oE 'role=[a-z]+' | sort -u | wc -l | tr -d ' ')
if [ "$roles" -ge 2 ]; then
  echo "  PASS: $roles roles receiving injection"
  ((PASS++))
else
  echo "  FAIL: only $roles role(s) receiving injection (expected >= 2)"
  ((FAIL++))
fi

# --- Test 6: Hook server socket is listening ---
echo "Test 6: Hook server socket is responding"
health=$(curl -s http://localhost:3380/health 2>/dev/null || echo "")
if [ -n "$health" ]; then
  echo "  PASS: hook server health endpoint responds"
  ((PASS++))
else
  # Try the socket path
  socket="/tmp/chorus-hooks.sock"
  if [ -S "$socket" ]; then
    echo "  PASS: hook server socket exists at $socket"
    ((PASS++))
  else
    echo "  FAIL: hook server not reachable (no HTTP health or socket)"
    ((FAIL++))
  fi
fi

# --- Test 7: LaunchAgent is KeepAlive ---
echo "Test 7: Hook server LaunchAgent is KeepAlive"
keepalive=$(plutil -p ~/Library/LaunchAgents/com.chorus.hooks.plist 2>/dev/null | grep -i keepalive)
if [ -n "$keepalive" ]; then
  echo "  PASS: KeepAlive configured"
  ((PASS++))
else
  echo "  FAIL: KeepAlive not set — hook server won't restart after crash"
  ((FAIL++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
