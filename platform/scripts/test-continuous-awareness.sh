#!/bin/bash
# test-continuous-awareness.sh — Tests for continuous awareness gate (#2003)
# AC: UserPromptSubmit does hybrid search, PostToolUse surfaces ops state

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0
HOOKS_LOG="$HOME/Library/Logs/Chorus/chorus-hooks.stdout.log"

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

echo "=== continuous awareness gate tests ==="
echo ""

# --- Test 1: Context injection fires on UserPromptSubmit ---
echo "Test 1: Context injection active on UserPromptSubmit"
recent=$(tail -100 "$HOOKS_LOG" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep 'context-inject.*event.*injected')
if [ -n "$recent" ]; then
  echo "  PASS: context injection firing"
  ((PASS++))
else
  echo "  FAIL: no context injection events"
  ((FAIL++))
fi

# --- Test 2: Hybrid search API returns results ---
echo "Test 2: Hybrid search returns results for known terms"
result=$(curl -s 'http://localhost:3340/api/chorus/search?q=compound+loop&mode=hybrid&limit=3' 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('total',0))" 2>/dev/null)
if [ -n "$result" ] && [ "$result" -gt 0 ] 2>/dev/null; then
  echo "  PASS: hybrid search returns $result results"
  ((PASS++))
else
  echo "  FAIL: hybrid search returned no results"
  ((FAIL++))
fi

# --- Test 3: Context injection covers multiple roles ---
echo "Test 3: Injection covers multiple roles"
roles=$(tail -200 "$HOOKS_LOG" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep 'context-inject.*injected' | grep -oE 'role=[a-z]+' | sort -u | wc -l | tr -d ' ')
if [ "$roles" -ge 2 ]; then
  echo "  PASS: $roles roles receiving injection"
  ((PASS++))
else
  echo "  FAIL: only $roles role(s) — expected >= 2"
  ((FAIL++))
fi

# --- Test 4: Memory scan returns hits ---
echo "Test 4: Memory scan finds related decisions/feedback"
memory_hits=$(tail -100 "$HOOKS_LOG" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep 'context-inject.*injected.*memory_hits=[1-9]' | tail -1)
if [ -n "$memory_hits" ]; then
  echo "  PASS: memory scan returning hits"
  ((PASS++))
else
  echo "  FAIL: no memory hits in recent injections"
  ((FAIL++))
fi

# --- Test 5: PostToolUse ops awareness is compiled and registered ---
echo "Test 5: ops_awareness module is compiled into hook server"
# The module is registered if the binary contains the ops-awareness string
if strings "$(dirname "$(dirname "$HOOKS_LOG")")"/../CascadeProjects/platform/services/chorus-hooks/target/release/chorus-hooks 2>/dev/null | grep -q "ops-awareness"; then
  echo "  PASS: ops_awareness compiled into binary"
  ((PASS++))
else
  # Fallback: check source registration
  if grep -q "ops_awareness" ${CHORUS_ROOT}/platform/services/chorus-hooks/src/hooks/mod.rs 2>/dev/null; then
    echo "  PASS: ops_awareness registered in mod.rs"
    ((PASS++))
  else
    echo "  FAIL: ops_awareness not found in binary or mod.rs"
    ((FAIL++))
  fi
fi

# --- Test 6: ops_awareness wired into PostToolUse ---
echo "Test 6: ops_awareness called in post_tool_use handler"
if grep -q "ops_awareness::check" ${CHORUS_ROOT}/platform/services/chorus-hooks/src/main.rs 2>/dev/null; then
  echo "  PASS: ops_awareness::check called in main.rs"
  ((PASS++))
else
  echo "  FAIL: ops_awareness::check not wired into PostToolUse"
  ((FAIL++))
fi

# --- Test 7: Hook server running ---
echo "Test 7: Hook server running"
pid=$(launchctl list com.chorus.hooks 2>/dev/null | grep '"PID"' | grep -o '[0-9]*')
if [ -n "$pid" ]; then
  echo "  PASS: hook server PID $pid"
  ((PASS++))
else
  echo "  FAIL: hook server not running"
  ((FAIL++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
