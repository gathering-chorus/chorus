#!/usr/bin/env bats
# Hermetic test for #2476 — three principles MCP tools (list/get/create).
# Tests against running chorus-api at :3340. Streamable HTTP transport
# requires session-init handshake before tools/list / tools/call.

MCP_URL="${MCP_URL:-http://localhost:3340/mcp}"

init_session() {
  curl -s -i -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"bats","version":"1.0"}}}' \
    | grep -i "^mcp-session-id:" | awk '{print $2}' | tr -d '\r\n'
}

ack_initialized() {
  local sess="$1"
  curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $sess" \
    -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' >/dev/null
}

@test "tools/list returns 4 tools — nudge + 3 principles" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
  echo "$resp" | grep -q '"chorus_nudge_message"'
  echo "$resp" | grep -q '"chorus_principles_list"'
  echo "$resp" | grep -q '"chorus_principles_get"'
  echo "$resp" | grep -q '"chorus_principles_create"'
}

@test "chorus_principles_list returns the live principle set" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"chorus_principles_list","arguments":{}}}')
  # hemenway-observe is one of the live 46 principles
  echo "$resp" | grep -q 'hemenway-observe'
}

@test "chorus_principles_get returns one named principle" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"chorus_principles_get","arguments":{"id":"hemenway-observe"}}}')
  echo "$resp" | grep -q 'Observe'
}

@test "chorus_principles_get rejects missing id" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"chorus_principles_get","arguments":{}}}')
  echo "$resp" | grep -qi 'invalid\|required\|error'
}

@test "chorus_principles_create rejects missing label" {
  SESS=$(init_session)
  ack_initialized "$SESS"
  resp=$(curl -s -X POST "$MCP_URL" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Mcp-Session-Id: $SESS" \
    -H "X-Chorus-Role: silas" \
    -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"chorus_principles_create","arguments":{}}}')
  echo "$resp" | grep -qi 'invalid\|label\|error'
}
