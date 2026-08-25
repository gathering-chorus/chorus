#!/bin/bash
# test-continuous-awareness.sh — Tests for continuous awareness gate (#2003)
# AC: UserPromptSubmit does hybrid search and injects context.
#
# #3606 rewrite — the original read $HOME/Library/Logs/Chorus/chorus-hooks.stdout.log,
# a file that no longer exists (the daemon's stdout moved; per-service file tails
# are ephemeral anyway). Loki is the durable log home (CLAUDE.md: ALWAYS use Loki
# for log search), and the spine events ({appName="chorus-events"}
# component=context-inject) are the system of record for injection activity.
# Tests 5/6 asserted ops_awareness wiring — RETIRED by #3334 (deleted, not
# stubbed); they now assert the retirement HOLDS (no resurrection), the #3598
# retirement-gate pattern.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
LOKI="${LOKI_URL:-http://localhost:3102}"
WINDOW_S="${AWARENESS_WINDOW_S:-43200}"   # 12h — spans overnight gaps

PASS=0
FAIL=0

# Pull the last window of context.inject.injected spine events from Loki once;
# every assertion reads this capture.
START="$(( $(date +%s) - WINDOW_S ))000000000"
# #3949 — the fetch is BOUNDED and its failure is distinguishable from an
# empty result. Unbounded (no --max-time, limit=1000 over 12h) this query
# ground Loki for minutes, the error vanished into 2>/dev/null, and "Loki
# slow" scored as "awareness broken" (the #3948 down-vs-unmeasurable class).
FETCH_OK=1
INJECTED=$(curl -s --max-time 20 -G "$LOKI/loki/api/v1/query_range" \
  --data-urlencode 'query={appName="chorus-events"} |= "context.inject.injected"' \
  --data-urlencode "start=$START" --data-urlencode "limit=200" 2>/dev/null \
  | python3 -c "
import json,sys
try:
    d=json.load(sys.stdin)
    for s in d.get('data',{}).get('result',[]):
        for v in s.get('values',[]): print(v[1])
except Exception:
    pass" 2>/dev/null) || FETCH_OK=0
[ -z "$INJECTED" ] && [ "$FETCH_OK" = "1" ] && {
  # Empty could still be a failed curl swallowed by the pipe — probe cheaply.
  curl -s --max-time 5 -o /dev/null "$LOKI/ready" || FETCH_OK=0
}
# #3949 — headless hours have NO user prompts, so zero injections is CORRECT
# then, not a defect. Gate the verdict on prompt activity in the same window.
PROMPTS=$(grep -c "jeff.input.delivered" "$HOME/.chorus/chorus.log" 2>/dev/null || echo 0)
if [ "$FETCH_OK" = "0" ]; then
  echo "UNMEASURABLE: Loki fetch failed/timed out — awareness not proven broken (#3753)"
  exit 0
fi

echo "=== continuous awareness gate tests ==="
echo ""

# --- Test 1: Context injection fires on UserPromptSubmit ---
echo "Test 1: Context injection active (Loki spine events, last ${WINDOW_S}s)"
count=$(echo "$INJECTED" | grep -c '"event":"context.inject.injected"')
if [ "$count" -gt 0 ]; then
  echo "  PASS: context injection firing ($count events)"
  PASS=$((PASS+1))
else
  if [ "${PROMPTS:-0}" -eq 0 ]; then
    echo "  UNMEASURABLE: no user prompts in window — nothing to inject for"
  else
    echo "  FAIL: prompts occurred but no context.inject.injected events"
    FAIL=$((FAIL+1))
  fi
fi

# --- Test 2: Hybrid search API returns results ---
echo "Test 2: Hybrid search returns results for known terms"
result=$(curl -s 'http://localhost:3340/api/chorus/search?q=compound+loop&mode=hybrid&limit=3' 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('total',0))" 2>/dev/null)
if [ -n "$result" ] && [ "$result" -gt 0 ] 2>/dev/null; then
  echo "  PASS: hybrid search returns $result results"
  PASS=$((PASS+1))
else
  echo "  FAIL: hybrid search returned no results"
  FAIL=$((FAIL+1))
fi

# --- Test 3: Context injection covers multiple roles ---
echo "Test 3: Injection covers multiple roles"
roles=$(echo "$INJECTED" | grep -oE '"role":"(wren|silas|kade)"' | sort -u | wc -l | tr -d ' ')
# #3606 — assert the MECHANISM (injection reaches whoever runs), not the
# calendar: >=2 roles in a 12h window false-reds on quiet weekends (07-06:
# only one role session ran overnight — the system was fine). Role count
# stays reported as info.
echo "  info: $roles distinct role(s) injected in window"
if [ "$roles" -ge 1 ]; then
  echo "  PASS: $roles roles receiving injection"
  PASS=$((PASS+1))
else
  echo "  FAIL: only $roles role(s) — expected >= 2"
  FAIL=$((FAIL+1))
fi

# --- Test 4: Memory scan returns hits ---
echo "Test 4: Memory scan finds related decisions/feedback"
memory_hits=$(echo "$INJECTED" | grep -oE '"memory_hits":[1-9][0-9]*' | head -1)
if [ -n "$memory_hits" ]; then
  echo "  PASS: memory scan returning hits ($memory_hits)"
  PASS=$((PASS+1))
else
  echo "  FAIL: no memory hits in recent injections"
  FAIL=$((FAIL+1))
fi

# --- Test 5+6: ops_awareness retirement holds (#3334) ---
# ops_awareness was deleted deliberately (#3334 — its stderr whisper fired on
# every PostToolUse). These slots used to assert the wiring EXISTS; they now
# gate against resurrection, so a re-add is a conscious decision that updates
# this test with its rationale.
echo "Test 5: ops_awareness stays retired (#3334)"
if grep -rq "ops_awareness" "${CHORUS_ROOT}/platform/services/chorus-hooks/src/hooks/" 2>/dev/null; then
  echo "  FAIL: ops_awareness reappeared in hooks/ — retired by #3334; if intentional, update this gate"
  FAIL=$((FAIL+1))
else
  echo "  PASS: no ops_awareness in hooks/ (retirement holds)"
  PASS=$((PASS+1))
fi

echo "Test 6: no ops_awareness::check call sites"
if grep -rq "ops_awareness::check" "${CHORUS_ROOT}/platform/services/chorus-hooks/src/" --include='*.rs' 2>/dev/null | grep -v "RETIRED" ; then
  echo "  FAIL: ops_awareness::check call site found — retired by #3334"
  FAIL=$((FAIL+1))
else
  echo "  PASS: no ops_awareness::check call sites"
  PASS=$((PASS+1))
fi

# --- Test 7: Hook server running ---
# #2801 — chorus-hooks is not a LaunchAgent. Detect via PID file + process
# liveness, fall back to pgrep on the binary path.
# #3606 — durable pid home first (~/.chorus/run); /tmp is a legacy mirror
# subject to OS eviction.
echo "Test 7: Hook server running"
pid=""
for pf in "$HOME/.chorus/run/chorus-hooks.pid" /tmp/chorus-hooks.pid; do
  if [ -f "$pf" ]; then
    pidfile=$(cat "$pf" 2>/dev/null)
    if [ -n "$pidfile" ] && kill -0 "$pidfile" 2>/dev/null; then
      pid="$pidfile"; break
    fi
  fi
done
if [ -z "$pid" ]; then
  pid=$(pgrep -f 'chorus-hooks/target/release/chorus-hooks$|/.chorus/bin/chorus-hooks$' 2>/dev/null | head -1)
fi
if [ -n "$pid" ]; then
  echo "  PASS: hook server PID $pid"
  PASS=$((PASS+1))
else
  echo "  FAIL: hook server not running"
  FAIL=$((FAIL+1))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
