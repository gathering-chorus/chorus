#!/usr/bin/env bats
# Hermetic test for #2472 — MCP transport + chorus_nudge_message tool.
# #2998: MCP moved from chorus-api:3340 to chorus-mcp:3341. Streamable HTTP
# transport requires session-init handshake before tools/list / tools/call.

MCP_URL="${MCP_URL:-http://localhost:3341/mcp}"
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SPINE_LOG="${CHORUS_LOG_FILE:-${CHORUS_ROOT}/platform/logs/chorus.log}"

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
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"chorus_nudge_message\",\"arguments\":{\"to\":\"silas\",\"message\":\"$PROBE\"}}}")
  echo "$resp" | grep -q '"result"' || (echo "no result: $resp" && false)
  echo "$resp" | grep -q "nudge sent: silas → silas" || (echo "wrong text: $resp" && false)
  # Give the spine emit a moment to land
  sleep 1
  tail -200 "$SPINE_LOG" | grep -q 'nudge.emitted' || (echo "no nudge.emitted in spine" && false)
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
