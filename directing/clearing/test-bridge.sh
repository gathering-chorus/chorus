#!/bin/bash
# Bridge integration tests — run before every deploy
# Jeff's rule: don't tell him to test until these pass

PASS=0
FAIL=0
BRIDGE="http://localhost:3470"

check() {
  local name="$1" result="$2"
  if [ "$result" = "true" ]; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== Bridge Integration Tests ==="
echo ""

# 1. Bridge is up
echo "--- Connectivity ---"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE" 2>/dev/null)
check "Bridge responds 200" "$([ "$STATUS" = "200" ] && echo true || echo false)"

HEALTH=$(curl -s "$BRIDGE/health" 2>/dev/null)
check "Health endpoint responds" "$([ -n "$HEALTH" ] && echo true || echo false)"

# 2. No XML tags in visible messages
echo ""
echo "--- Noise Filtering ---"
MESSAGES=$(curl -s "$BRIDGE/api/messages" 2>/dev/null)
XML_NOISE=$(echo "$MESSAGES" | python3 -c "
import json,sys
msgs = json.load(sys.stdin)
noise = [m for m in msgs if '<task-notification>' in m.get('text','') or '<system-reminder>' in m.get('text','') or '<command-message>' in m.get('text','')]
print(len(noise))
" 2>/dev/null)
check "No XML tags in visible messages" "$([ "$XML_NOISE" = "0" ] && echo true || echo false)"

# 3. No file paths in visible messages
# #4277 — the "no raw file paths in visible messages" check is retired: it
# contradicted proving/flows/clearing-ui.spec.cjs ("a reply that quotes a
# filesystem path reaches Jeff's room"), which posts exactly such a message on
# every nightly to prove the router does NOT hide real replies. Both ran in the
# 2026-09-23 14:34 nightly and this one went red on the other's canary. A path
# in a real reply is content; machinery is caught by name in the router.
echo "  SKIP: raw-path check retired (#4277) — a reply quoting a path is content, see clearing-ui.spec.cjs"

# 4. No tool metadata suffixes
TOOL_NOISE=$(echo "$MESSAGES" | python3 -c "
import json,sys
msgs = json.load(sys.stdin)
noise = [m for m in msgs if '| tools:' in m.get('text','') and m.get('visible',False)]
print(len(noise))
" 2>/dev/null)
check "No '| tools: X | 0.0s' suffixes in visible messages" "$([ "$TOOL_NOISE" = "0" ] && echo true || echo false)"

# 5. No duplicate messages (same body within 5 seconds)
echo ""
echo "--- Deduplication ---"
DUPES=$(echo "$MESSAGES" | python3 -c "
import json,sys
msgs = json.load(sys.stdin)
seen = {}
dupes = 0
for m in msgs:
  key = m.get('text','')[:100]
  ts = m.get('ts','')
  if key in seen and abs(hash(ts) - hash(seen[key])) < 10:
    dupes += 1
  seen[key] = ts
print(dupes)
" 2>/dev/null)
check "No duplicate messages" "$([ "$DUPES" = "0" ] && echo true || echo false)"

# 6. WebSocket connected
echo ""
echo "--- Socket ---"
DEBUG=$(curl -s "$BRIDGE/api/debug" 2>/dev/null)
CLIENTS=$(echo "$DEBUG" | python3 -c "import json,sys; print(json.load(sys.stdin).get('connectedClients',0))" 2>/dev/null)
# #4255 — "is anyone attached right now" is not a property of the bridge. This
# assertion passed or failed depending on whether a browser happened to be open
# when the suite ran: three runs on 2026-09-20/21 gave three different answers.
# What IS testable is that the bridge REPORTS its client count.
check "bridge reports a client count" "$([ -n "$CLIENTS" ] && [ "$CLIENTS" -ge 0 ] 2>/dev/null && echo true || echo false)"
echo "   (clients attached right now: ${CLIENTS:-unknown} — reported, not asserted)"

# 7. All 3 role sessions discovered
echo ""
echo "--- Session Discovery ---"
SESSIONS=$(echo "$DEBUG" | python3 -c "import json,sys; print(json.load(sys.stdin).get('sessionCount',0))" 2>/dev/null)
check "3 role sessions discovered" "$([ "$SESSIONS" -ge 3 ] 2>/dev/null && echo true || echo false)"

# 8. Graceful restart
#
# #4255 — this used to POST /api/restart to the LIVE Clearing and assert it came
# back. Running the test restarted the room the team talks in, and the result
# depended on who was connected at that second. Jeff, 2026-09-20: a test that
# mutates prod cannot tell "bridge broken" from "nobody was attached".
#
# Restarting is real behaviour and worth covering — but against a bridge the
# test starts and owns, not this one. Opt in with CLEARING_RESTART_TEST=1 and a
# BRIDGE that is not the live port.
echo ""
echo "--- Restart ---"
if [ "${CLEARING_RESTART_TEST:-}" = "1" ] && [ "$BRIDGE" != "http://localhost:3470" ]; then
  RESTART=$(curl -s -X POST "$BRIDGE/api/restart" 2>/dev/null)
  sleep 3
  POST_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE" 2>/dev/null)
  check "Graceful restart returns 200 after" "$([ "$POST_STATUS" = "200" ] && echo true || echo false)"
else
  echo "   SKIPPED — restart is not exercised against the live bridge (#4255)."
  echo "   Set CLEARING_RESTART_TEST=1 with BRIDGE pointed at a test instance."
fi

# Summary
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -gt 0 ]; then
  echo "BLOCKED — fix failures before demo"
  exit 1
else
  echo "ALL PASS — safe to demo"
  exit 0
fi
