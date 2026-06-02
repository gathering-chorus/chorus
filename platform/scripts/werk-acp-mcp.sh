#!/usr/bin/env bash
# werk-acp-mcp.sh — drive the atomic werk verbs through the REAL MCP HTTP path,
# in sequence, the same flow the agents exercise. No rust binaries directly, no
# hand-rolled verb logic — every step is a tools/call against the live chorus-api
# MCP server (http://localhost:$CHORUS_MCP_PORT/mcp). Dies LOUD at the first fail.
#
# Usage:
#   werk-acp-mcp.sh <builder-role> <card> [accepter] [target]
#     builder-role  kade|wren|silas   whose werk is being landed
#     card          card id
#     accepter      jeff|wren  who finalizes (DEC-048); default jeff; must != builder
#     target        prod deploy target; default canonical
#
# Sequence — demo is a PREREQUISITE for prod, so both run, demo first:
#   werk-commit -> werk-push
#     -> chorus_deploy(target=werk)        DEMO  build+deploy to the role's slot
#     -> chorus_deploy(target=canonical)   PROD  build+deploy+verify to canonical
#     -> werk-accept (called AS the accepter, role-arg stays the builder)
#
# MCP: Streamable-HTTP. initialize (capture Mcp-Session-Id) -> notifications/initialized
# -> tools/call. X-Chorus-Role is read per-request, so accept can run as a different
# role over the same session. Responses are SSE-framed (`data: <json>`).

set -uo pipefail

BUILDER="${1:?usage: werk-acp-mcp.sh <builder-role> <card> [accepter] [target]}"
CARD="${2:?usage: werk-acp-mcp.sh <builder-role> <card> [accepter] [target]}"
ACCEPTER="${3:-jeff}"
TARGET="${4:-canonical}"

PORT="${CHORUS_MCP_PORT:-3341}"
URL="http://localhost:${PORT}/mcp"
RPC_ID=0

case "$BUILDER"  in kade|wren|silas) ;; *) echo "FATAL: builder must be kade|wren|silas (got '$BUILDER')"  >&2; exit 2 ;; esac
case "$ACCEPTER" in jeff|wren|kade|silas) ;; *) echo "FATAL: accepter must be jeff|wren|kade|silas (got '$ACCEPTER')" >&2; exit 2 ;; esac
[[ "$ACCEPTER" == "jeff" || "$ACCEPTER" != "$BUILDER" ]] || { echo "FATAL: $ACCEPTER cannot self-accept own card (DEC-048)" >&2; exit 2; }
[[ "$CARD" =~ ^[0-9]+$ ]] || { echo "FATAL: card must be a number (got '$CARD')" >&2; exit 2; }

# --- handshake: one session for the whole chain ----------------------------
SID="$(curl -sS -D - -o /dev/null -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $BUILDER" \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"werk-acp-mcp","version":"1.0"}}}' \
  | awk -F': ' 'tolower($1)=="mcp-session-id"{gsub(/\r/,"",$2);print $2}')"
[[ -n "$SID" ]] || { echo "FATAL: no MCP session id — is chorus-api up on :$PORT ?" >&2; exit 1; }
curl -sS -o /dev/null -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $BUILDER" \
  -H "Mcp-Session-Id: $SID" -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'

# --- mcp_call <calling-role> <tool> <args-json> : one tools/call, fail loud --
mcp_call() {
  local role="$1" tool="$2" args="$3" body resp data
  RPC_ID=$((RPC_ID + 1))
  body="$(printf '{"jsonrpc":"2.0","id":%d,"method":"tools/call","params":{"name":"%s","arguments":%s}}' "$RPC_ID" "$tool" "$args")"
  resp="$(curl -sS -X POST "$URL" \
    -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $role" \
    -H "Mcp-Session-Id: $SID" -d "$body")"
  data="$(printf '%s' "$resp" | sed -n 's/^data: //p' | tail -1)"
  [[ -n "$data" ]] || { echo "  <no response from MCP>" >&2; return 1; }
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
}

step() {  # step <label> <calling-role> <tool> <args-json>
  echo "-- $1  (as $2 -> $3) --------------------------------"
  if mcp_call "$2" "$3" "$4"; then echo "   [ok] $1"
  else echo "   [FAIL] $1 — chain stops here. Nothing downstream ran." >&2; exit 1; fi
}

echo "werk-acp-mcp: builder=$BUILDER card=$CARD accepter=$ACCEPTER target=$TARGET  via $URL"
echo

step "commit"      "$BUILDER"  werk-commit   "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"
step "push"        "$BUILDER"  werk-push     "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"
step "demo-deploy" "$BUILDER"  chorus_deploy "$(printf '{"role":"%s","card_id":%s,"target":"werk"}' "$BUILDER" "$CARD")"
step "prod-deploy" "$BUILDER"  chorus_deploy "$(printf '{"role":"%s","card_id":%s,"target":"%s"}' "$BUILDER" "$CARD" "$TARGET")"
step "accept"      "$ACCEPTER" werk-accept   "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"

echo
echo "[done] all steps green — #$CARD landed via the live MCP flow."
