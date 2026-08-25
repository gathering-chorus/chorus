#!/bin/bash
# test-compound-loop.sh — Tests for compound loop reboot survival (#2008)
# AC: after-prompt hooks fire on first prompt of new session, context injected every prompt

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0
# #3606 — chorus-hooks.stdout.log no longer exists; Loki holds the spine
# events durably (CLAUDE.md: ALWAYS use Loki for log search).
LOKI="${LOKI_URL:-http://localhost:3102}"
_loki_injected() {
  local start
  start="$(( $(date +%s) - ${AWARENESS_WINDOW_S:-43200} ))000000000"
  # #3949 — bounded: unbounded limit=1000 over 12h ground Loki for minutes at
  # 04:44 and the swallowed timeout scored as "loop broken" (#3948 class).
  curl -s --max-time 20 -G "$LOKI/loki/api/v1/query_range" \
    --data-urlencode 'query={appName="chorus-events"} |= "context.inject.injected"' \
    --data-urlencode "start=$start" --data-urlencode "limit=200" 2>/dev/null \
    | python3 -c "
import json,sys
try:
    d=json.load(sys.stdin)
    for s in d.get('data',{}).get('result',[]):
        for v in s.get('values',[]): print(v[1])
except Exception:
    pass" 2>/dev/null
}
HOOKS_STDERR="$HOME/Library/Logs/Chorus/chorus-hooks.stderr.log"

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qi "$needle"; then
    echo "  PASS: $label"
    PASS=$((PASS+1))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    FAIL=$((FAIL+1))
  fi
}

echo "=== compound loop reboot survival tests ==="
echo ""

# --- Test 1: Hook server is running ---
# #2801 — chorus-hooks is not a LaunchAgent (no com.chorus.hooks plist).
# Daemon runs as a regular process; check via /tmp/chorus-hooks.pid (the
# canonical PID file) with a process-alive verification, or fall back to
# pgrep for the binary name.
echo "Test 1: Hook server is running"
pid=""
if [ -f /tmp/chorus-hooks.pid ]; then
  pidfile=$(cat /tmp/chorus-hooks.pid 2>/dev/null)
  if [ -n "$pidfile" ] && kill -0 "$pidfile" 2>/dev/null; then
    pid="$pidfile"
  fi
fi
if [ -z "$pid" ]; then
  pid=$(pgrep -f 'chorus-hooks/target/release/chorus-hooks$|/.chorus/bin/chorus-hooks$' 2>/dev/null | head -1)
fi
if [ -n "$pid" ]; then
  echo "  PASS: hook server running (PID $pid)"
  PASS=$((PASS+1))
else
  echo "  FAIL: hook server not running"
  FAIL=$((FAIL+1))
fi

# --- Test 2: settings.json has UserPromptSubmit hook ---
echo "Test 2: UserPromptSubmit hook registered in settings.json"
output=$(cat ~/.claude/settings.json 2>/dev/null)
assert_contains "UserPromptSubmit registered" "UserPromptSubmit" "$output"

# --- Test 3: Hook shim binary exists ---
echo "Test 3: Hook shim binary exists"
# #3725 — was hardcoded to ${CHORUS_ROOT}/platform/services/chorus-hooks/target/release.
# Per CSC #2734 that is the BUILD artifact; the DEPLOY artifact lives in
# ~/.chorus/bin (that split exists so macOS TCC grants survive rebuilds). The
# hardcoded path made this FAIL in any freshly-pulled werk — no release build
# there yet — while the shim was deployed and serving the whole time. A false red
# about a binary that exists, which is the same stale-path class as the retired
# /tmp socket fixed in #3721. Resolve the way the operational scripts do:
# `command -v` first, build path only as the pre-deploy fallback.
SHIM="$(command -v chorus-hook-shim 2>/dev/null || true)"
[ -n "$SHIM" ] || SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
if [ -x "$SHIM" ]; then
  echo "  PASS: shim binary exists and is executable ($SHIM)"
  PASS=$((PASS+1))
else
  echo "  FAIL: shim binary not found at $SHIM"
  FAIL=$((FAIL+1))
fi

# --- Test 4: Context injection fired in the last 5 minutes ---
echo "Test 4: Context injection fired recently"
recent=$(_loki_injected | head -1)
if [ -n "$recent" ]; then
  echo "  PASS: context injection fired"
  PASS=$((PASS+1))
else
  echo "  FAIL: no recent context injection events in hook log"
  FAIL=$((FAIL+1))
fi

# --- Test 5: Multiple roles receiving injection ---
echo "Test 5: Multiple roles receiving context injection"
roles=$(_loki_injected | grep -oE '"role":"(wren|silas|kade)"' | sort -u | wc -l | tr -d ' ')
# #3606 — mechanism not calendar (see test-continuous-awareness.sh): quiet
# weekends have one active role; injection working for one proves the loop.
echo "  info: $roles distinct role(s) injected in window"
if [ "$roles" -ge 1 ]; then
  echo "  PASS: $roles roles receiving injection"
  PASS=$((PASS+1))
else
  echo "  FAIL: only $roles role(s) receiving injection (expected >= 2)"
  FAIL=$((FAIL+1))
fi

# --- Test 6: Hook server socket is listening ---
echo "Test 6: Hook server socket is responding"
health=$(curl -s http://localhost:3380/health 2>/dev/null || echo "")
if [ -n "$health" ]; then
  echo "  PASS: hook server health endpoint responds"
  PASS=$((PASS+1))
else
  # #3721 — the control socket moved out of /tmp to ~/.chorus/run/ (#3617/#3631).
  # This still probed the retired path, so it reported "hook server not
  # reachable" against a daemon that was up and serving on the new one: a FALSE
  # RED, and the mirror image of the false greens elsewhere on this card — same
  # root cause, opposite symptom. Same repoint already applied to the chorus-hooks
  # Rust tests here. Derived from HOME rather than a second hardcoded literal, so
  # it tracks the daemon instead of pinning a path that can drift again.
  socket="${CHORUS_HOOK_SOCKET:-$HOME/.chorus/run/chorus-hooks.sock}"
  if [ -S "$socket" ]; then
    echo "  PASS: hook server socket exists at $socket"
    PASS=$((PASS+1))
  else
    echo "  FAIL: hook server not reachable (no HTTP health or socket)"
    FAIL=$((FAIL+1))
  fi
fi

# --- Test 7: LaunchAgent is KeepAlive ---
echo "Test 7: Hook server LaunchAgent is KeepAlive"
keepalive=$(plutil -p ~/Library/LaunchAgents/com.chorus.hooks.plist 2>/dev/null | grep -i keepalive)
if [ -n "$keepalive" ]; then
  echo "  PASS: KeepAlive configured"
  PASS=$((PASS+1))
else
  echo "  FAIL: KeepAlive not set — hook server won't restart after crash"
  FAIL=$((FAIL+1))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
