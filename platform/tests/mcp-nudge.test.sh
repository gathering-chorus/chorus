#!/usr/bin/env bats
# @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
: "${CHORUS_ROOT:=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)}"
# Hermetic test for #2472 — MCP transport + chorus_nudge_message tool.
# #2998: MCP moved from chorus-api:3340 to chorus-mcp:3341. Streamable HTTP
# transport requires session-init handshake before tools/list / tools/call.

MCP_URL="${MCP_URL:-http://localhost:3341/mcp}"
CHORUS_ROOT="${CHORUS_ROOT:-${CHORUS_ROOT}}"
# The spine is ~/.chorus/chorus.log and has been since 2026-05-04, when branch
# checkouts were clobbering the unstaged repo-local file mid-write.
#
# #4113 (Wren) and #4111 (Kade) found this the same week. This suite defaulted to
# ${CHORUS_ROOT}/platform/logs/chorus.log — a dead reader. The nudge WAS emitted
# every night; the test was looking in a file nothing writes to and reporting
# "no nudge.emitted in spine" about a spine it was not reading. Measured: that file
# holds zero nudge.emitted, the real spine holds this test's own probe.
#
# A stale file that still EXISTS is worse than a missing one — it can hold a fossil
# event from before the move and let the assertion pass for a nudge sent in April.
# So a missing spine is UNMEASURED and refuses; it never reads as green.
SPINE_LOG="${CHORUS_LOG_FILE:-$HOME/.chorus/chorus.log}"
[ -s "$SPINE_LOG" ] || { echo "UNMEASURED: no spine at $SPINE_LOG — emits cannot be verified"; exit 1; }

# Helper: initialize and capture session id
init_session() {
  curl -s -i -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"bats","version":"1.0"}}}' \
    | grep -i "^mcp-session-id:" | awk '{print $2}' | tr -d '\r\n'
}

# Helper: send notifications/initialized so the session is ready for tools
ack_initialized() {
  local sess="$1"
  curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $sess" \
    -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' >/dev/null
}

@test "MCP endpoint responds to initialize and returns session id" {
  SESS=$(init_session)
  [ -n "$SESS" ] || (echo "no session id returned" && false)
}

@test "tools/list includes chorus_nudge_message with typed schema" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
  echo "$resp" | grep -q 'chorus_nudge_message' || (echo "tool not in list: $resp" && false)
  echo "$resp" | grep -q 'inputSchema' || (echo "no inputSchema: $resp" && false)
  echo "$resp" | grep -qE '"enum":\["silas","wren","kade","jeff"\]' || (echo "wrong target enum: $resp" && false)
}

@test "tools/call chorus_nudge_message returns success and emits spine event" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  PROBE="MCP-HERMETIC-TEST-$(date +%s)"
  # #4111 — remember where the spine was BEFORE the call. The assertion below
  # used `tail -200`, which is a bet on how busy the log is: this spine writes
  # 3,000 lines in well under a minute on a working day, so the event scrolled
  # out of the window before the grep ran and the test called a working nudge a
  # failure. A byte offset is not a bet.
  SPINE_MARK=$(wc -c < "$SPINE_LOG" 2>/dev/null || echo 0)
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"chorus_nudge_message\",\"arguments\":{\"to\":\"silas\",\"message\":\"$PROBE\"}}}")
  echo "$resp" | grep -q '"result"' || (echo "no result: $resp" && false)
  echo "$resp" | grep -q "nudge sent: silas → silas" || (echo "wrong text: $resp" && false)
  # #4130 — WAIT for the emit instead of betting one second covers it. The
  # 2026-09-09 03:00 nightly (load 35) saw the call answer "nudge sent" and the
  # spine line land AFTER the grep ran: a working nudge called red. Poll up to
  # 20s for THIS probe; a nudge that never emits still reds at the cap.
  for _i in $(seq 1 40); do
    tail -c "+$((SPINE_MARK + 1))" "$SPINE_LOG" | grep -q "$PROBE" && break
    sleep 0.5
  done
  # Search only what was written since the mark, and match THIS run's probe.
  # Grepping the bare event name would pass on any nudge any role happened to
  # send while this test ran — it could not tell its own emit from a neighbour's,
  # which is the other half of why the old assertion was untrustworthy.
  tail -c "+$((SPINE_MARK + 1))" "$SPINE_LOG" | grep -q "nudge.emitted" \
    || (echo "no nudge.emitted written since the call" && false)
  tail -c "+$((SPINE_MARK + 1))" "$SPINE_LOG" | grep -q "$PROBE" \
    || (echo "nudge.emitted found but not THIS probe ($PROBE)" && false)
}

@test "tools/call rejects invalid target role" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"chorus_nudge_message","arguments":{"to":"bob","message":"hi"}}}')
  echo "$resp" | grep -qE '"error"|Invalid arguments' || (echo "bad role accepted: $resp" && false)
}

@test "tools/call rejects empty message" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"chorus_nudge_message","arguments":{"to":"silas","message":""}}}')
  echo "$resp" | grep -qE '"error"|Invalid arguments' || (echo "empty msg accepted: $resp" && false)
}
