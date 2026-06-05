#!/usr/bin/env bash
# chorus-mcp-call.sh — #3236. ONE MCP tools/call, fail-loud, self-contained.
#
# The single toolchain. Every pipeline step (act workflow OR bash) invokes a verb
# THROUGH this helper, never as a bare binary. Decided 2026-06-04 (Jeff): MCP is the
# one toolchain; act is the one orchestrator. This is the seam that makes the collapse
# coherent — extracted verbatim from werk-mcp.sh's mcp_call so the bash and act paths
# share identical transport, SSE parsing, and error contract (no drift).
#
# Self-contained per invocation: inits its own MCP session then makes one tools/call.
# Unlike werk-mcp.sh (one bash process, one shared session across calls), act runs each
# step in a SEPARATE shell — so a shared session var can't cross steps. Per-call init is
# the correct shape for the act orchestrator; the overhead is one extra round-trip.
#
# Usage:   chorus-mcp-call.sh <role> <tool> <args-json>
# Example: chorus-mcp-call.sh kade werk-commit '{"role":"kade","card_id":3236}'
# Exit:    0 on tool success; 1 on JSON-RPC error, isError result, no response, or
#          no session (chorus-api down). Tool text output goes to stdout.

set -uo pipefail

ROLE="${1:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"
TOOL="${2:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"
ARGS="${3:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"

PORT="${CHORUS_MCP_PORT:-3341}"
URL="http://localhost:${PORT}/mcp"

# --- handshake: a fresh session for this one call --------------------------
SID="$(curl -sS -D - -o /dev/null -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $ROLE" \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"chorus-mcp-call","version":"1.0"}}}' \
  | awk -F': ' 'tolower($1)=="mcp-session-id"{gsub(/\r/,"",$2);print $2}')"
[[ -n "$SID" ]] || { echo "FATAL: no MCP session id — is chorus-api up on :$PORT ?" >&2; exit 1; }
curl -sS -o /dev/null -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $ROLE" \
  -H "Mcp-Session-Id: $SID" -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'

# --- one tools/call ---------------------------------------------------------
body="$(printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"%s","arguments":%s}}' "$TOOL" "$ARGS")"
resp="$(curl -sS -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $ROLE" \
  -H "Mcp-Session-Id: $SID" -d "$body")"
data="$(printf '%s' "$resp" | sed -n 's/^data: //p' | tail -1)"
[[ -n "$data" ]] || { echo "  <no response from MCP>" >&2; exit 1; }
printf '%s' "$data" | python3 -c '
import sys, json
d = json.load(sys.stdin)
if "error" in d:
    print("  JSON-RPC ERROR:", json.dumps(d["error"])); sys.exit(1)
r = d.get("result", {})
txt = "\n".join(c.get("text","") for c in r.get("content", []) if c.get("type")=="text")
print("  " + (txt or json.dumps(r)).replace("\n","\n  "))
sys.exit(1 if r.get("isError") else 0)
'
