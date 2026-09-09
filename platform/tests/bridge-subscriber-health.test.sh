#!/usr/bin/env bash
# @test-type: integration — operational; live services, skip-if-absent in CI
: "${CHORUS_ROOT:=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)}"
# bridge-subscriber-health.test.sh — Verify bridge subscribers stay connected and deliver events
# Card #1964 AC: no ping timeout disconnects, ThrottleInterval set, end-to-end delivery
# Run: bash platform/tests/bridge-subscriber-health.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

echo "=== Bridge Subscriber Health Tests (#1964) ==="

# --- AC #1: Subscribers stay connected without ping timeout disconnects ---

# Test 1: All 3 subscribers are running (have PIDs)
for role in silas wren kade; do
  pid=$(launchctl list 2>/dev/null | grep "com.chorus.bridge-subscriber-$role" | awk '{print $1}')
  if [ -n "$pid" ] && [ "$pid" != "-" ]; then
    test_pass "bridge-subscriber-$role is running (PID $pid)"
  else
    test_fail "bridge-subscriber-$role is not running"
  fi
done

# Test 2: no ping timeout in the last PING_WINDOW_MINS (recent stability)
#
# #4130 — this asked "in the last 20 LINES" because the log carried no times.
# #1964 fixed the ping settings themselves (server 60s / client 120s) and this
# check was written to watch that fix hold; the line window is what makes it
# unable to. A line window cannot age out. After the launchd fix took this
# suite from 6 fails to 3, the remaining 3 were timeouts from BEFORE that fix,
# still inside the last 20 lines — and a healthy subscriber is a quiet one, so
# it never writes the lines that would push them past the edge. The healthier
# it got, the longer it stayed red.
#
# bridge-subscriber.js now stamps every line, so the window is time, which is
# what "recent" meant all along.
PING_WINDOW_MINS="${PING_WINDOW_MINS:-60}"
_recent_ping_timeouts() { # $1=log → count of stamped ping timeouts inside the window
  local log="$1" cutoff
  cutoff="$(date -u -v-"${PING_WINDOW_MINS}"M '+%Y-%m-%dT%H:%M:%S' 2>/dev/null)" || return 1
  # Stamped lines only. A line without a timestamp predates #4130 and cannot be
  # dated; counting it would resurrect exactly the bug being removed, and
  # skipping it is honest — an undatable line is not evidence of a RECENT
  # timeout. ISO-8601 UTC sorts lexically, so a string compare is a time compare.
  grep 'ping timeout' "$log" 2>/dev/null \
    | grep -oE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:]{8}' \
    | awk -v c="$cutoff" '$0 >= c' | wc -l | tr -d ' '
}
for role in silas wren kade; do
  log="$HOME/Library/Logs/Chorus/bridge-subscriber-$role.log"
  if [ -f "$log" ]; then
    recent_disconnects="$(_recent_ping_timeouts "$log")"
    if [ "${recent_disconnects:-0}" -eq 0 ]; then
      test_pass "bridge-subscriber-$role: no ping timeouts in the last ${PING_WINDOW_MINS}m"
    else
      test_fail "bridge-subscriber-$role: $recent_disconnects ping timeouts in the last ${PING_WINDOW_MINS}m"
    fi
  else
    test_fail "bridge-subscriber-$role: no log file"
  fi
done

# --- AC #2: ThrottleInterval set on LaunchAgent plists ---

for role in silas wren kade; do
  plist="$HOME/Library/LaunchAgents/com.chorus.bridge-subscriber-$role.plist"
  if [ -f "$plist" ]; then
    throttle=$(/usr/libexec/PlistBuddy -c "Print :ThrottleInterval" "$plist" 2>/dev/null || echo "MISSING")
    if [ "$throttle" != "MISSING" ] && [ "$throttle" -gt 0 ] 2>/dev/null; then
      test_pass "bridge-subscriber-$role plist has ThrottleInterval=$throttle"
    else
      test_fail "bridge-subscriber-$role plist missing ThrottleInterval"
    fi
  else
    test_fail "bridge-subscriber-$role plist not found"
  fi
done

# --- AC #3: Client ping/timeout settings aligned with server ---

SUBSCRIBER="${CHORUS_ROOT}/platform/scripts/bridge-subscriber.js"
SERVER="${CHORUS_ROOT}/directing/clearing/src/server.ts"

# Test 8: Client pingTimeout > server pingInterval
client_ping_timeout=$(grep -o 'pingTimeout: [0-9]*' "$SUBSCRIBER" | grep -o '[0-9]*')
server_ping_interval=$(grep -o 'pingInterval: [0-9]*' "$SERVER" | grep -o '[0-9]*')

if [ -n "$server_ping_interval" ]; then
  if [ -n "$client_ping_timeout" ] && [ "$client_ping_timeout" -gt "$server_ping_interval" ]; then
    test_pass "Client pingTimeout ($client_ping_timeout) > server pingInterval ($server_ping_interval)"
  else
    test_fail "Client pingTimeout ($client_ping_timeout) not > server pingInterval ($server_ping_interval)"
  fi
else
  test_fail "Server has no explicit pingInterval — using default 25s, prone to missed pings"
fi

# --- AC #4: End-to-end delivery ---

# Test 9: Bridge is reachable
if curl -sf --max-time 3 http://localhost:3470/health > /dev/null 2>&1; then
  test_pass "Bridge (localhost:3470) is reachable"
else
  test_fail "Bridge (localhost:3470) not reachable — can't test delivery"
fi

# Test 10: Subscribers are connected and listening on the board-event channel
# Board events come from the tailer, not a POST endpoint.
# Verify the Socket.IO connection is alive by checking the subscriber
# hasn't disconnected since its last "Connected" message.
connected=true
for role in silas wren kade; do
  log="$HOME/Library/Logs/Chorus/bridge-subscriber-$role.log"
  if [ -f "$log" ]; then
    last_line=$(tail -1 "$log")
    if echo "$last_line" | grep -q "Connected"; then
      : # good
    elif echo "$last_line" | grep -q "Disconnected\|error\|Shutting down"; then
      connected=false
      test_fail "bridge-subscriber-$role: last log line is disconnect/error"
    fi
  fi
done
if $connected; then
  test_pass "All subscribers connected and stable (no disconnect after last connect)"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
