#!/usr/bin/env bash
# werk-mcp.sh — drive the 8-step werk flow through the REAL MCP HTTP path, in order,
# the same calls the agents exercise. No rust binaries directly, no hand-rolled verb
# logic — every step is a tools/call against the live chorus-api MCP server
# (http://localhost:$CHORUS_MCP_PORT/mcp). Dies LOUD on the named step at first fail.
#
# Usage:
#   werk-mcp.sh <builder-role> <card> [accepter]
#     builder-role  kade|wren|silas   whose werk is being landed
#     card          card id
#     accepter      jeff|wren  who finalizes (DEC-048); default jeff; must != builder
#
# THE 8-STEP FLOW (per /tmp/werk-value-stream-v2.svg, Jeff's spec 2026-06-03):
#   — werk-demo boundary: prove in the role's slot, never touches main —
#   1 werk-commit   (rebase onto origin/main happens INSIDE commit, #3186)
#   2 werk-push
#   3 chorus_build  (build for the demo test)
#   4 chorus_deploy target=werk + chorus_env_up (stand up the running test instance)
#   — werk-acp boundary: land to prod; prod is a build of MAIN —
#   5 werk-merge    [interim] no verb yet (#3175) — merge to main via gh pr merge
#   6 chorus_build  MUST build from MAIN
#   7 chorus_deploy target=canonical, from MAIN
#   8 werk-accept   X-Chorus-Role=accepter (jeff/wren); role-arg stays the builder
#
# Steps 1-5 run for real. Step 6 HARD-STOPS at the deploy-from-main gap: chorus_build/
# chorus_deploy build from the WERK, and deploying werk-content to canonical IS the
# merged≠live bug — so we REFUSE rather than fake it. Steps 6-8 are the intended flow,
# gated behind the stop until deploy-from-main exists. demo-from-werk (3-4) is correct
# (your slot); prod-from-werk (6-7) is not (it's not main) — that asymmetry is the gate.
#
# MCP: Streamable-HTTP. initialize (capture Mcp-Session-Id) -> notifications/initialized
# -> tools/call. X-Chorus-Role is read per-request, so accept runs as a different role
# over the same session. Responses are SSE-framed (`data: <json>`).

set -uo pipefail

BUILDER="${1:?usage: werk-mcp.sh <builder-role> <card> [accepter]}"
CARD="${2:?usage: werk-mcp.sh <builder-role> <card> [accepter]}"
ACCEPTER="${3:-jeff}"

PORT="${CHORUS_MCP_PORT:-3341}"
URL="http://localhost:${PORT}/mcp"
WERK="${CHORUS_WERK_BASE:-$HOME/CascadeProjects/chorus-werk}/${BUILDER}-${CARD}"
BRANCH="${BUILDER}/${CARD}"
RPC_ID=0

case "$BUILDER"  in kade|wren|silas) ;; *) echo "FATAL: builder must be kade|wren|silas (got '$BUILDER')"  >&2; exit 2 ;; esac
case "$ACCEPTER" in jeff|wren|kade|silas) ;; *) echo "FATAL: accepter must be jeff|wren|kade|silas (got '$ACCEPTER')" >&2; exit 2 ;; esac
[[ "$ACCEPTER" == "jeff" || "$ACCEPTER" != "$BUILDER" ]] || { echo "FATAL: $ACCEPTER cannot self-accept own card (DEC-048)" >&2; exit 2; }
[[ "$CARD" =~ ^[0-9]+$ ]] || { echo "FATAL: card must be a number (got '$CARD')" >&2; exit 2; }

# --- handshake: one session for the whole chain ----------------------------
SID="$(curl -sS -D - -o /dev/null -X POST "$URL" \
  -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -H "X-Chorus-Role: $BUILDER" \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"werk-mcp","version":"1.0"}}}' \
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

echo "werk-mcp: builder=$BUILDER card=$CARD accepter=$ACCEPTER  via $URL"
echo

# ═══ werk-demo boundary — prove in the role's slot, never touches main ═══
step "1 commit"      "$BUILDER"  werk-commit   "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"
step "2 push"        "$BUILDER"  werk-push     "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"
step "3 build-demo"  "$BUILDER"  chorus_build  "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"
step "4 deploy-demo" "$BUILDER"  chorus_deploy "$(printf '{"role":"%s","card_id":%s,"target":"werk"}' "$BUILDER" "$CARD")"
step "4 env-up"      "$BUILDER"  chorus_env_up "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"

# ═══ werk-acp boundary — land to prod; prod is a build of MAIN ═══
# Step 5 — werk-merge: NO VERB YET (#3175). Interim: a real merge to main via gh pr merge.
echo "-- 5 merge  ([interim] werk-merge not built #3175 — merging via gh pr merge) -----"
if ( cd "$WERK" && gh pr merge "$BRANCH" --merge ); then
  echo "   [ok] 5 merge ([interim] gh pr merge — replace with the werk-merge verb, #3175)"
else
  echo "   [FAIL] 5 merge — gh pr merge failed (interim path). Chain stops." >&2; exit 1
fi

# Step 6 — werk-build for PROD, from MAIN. HARD-STOP: deploy-from-main is not built.
cat >&2 <<'BLOCKED'
-- 6 build-prod / 7 deploy-prod / 8 accept ------------------------------------
   [BLOCKED] deploy-from-main not built — refusing to deploy werk-content to prod.
   chorus_build/chorus_deploy build from the WERK, not MAIN; deploying that to
   canonical is the merged≠live bug. The card is MERGED (step 5); the prod deploy
   waits for deploy-from-main. Steps 6 (build-from-main), 7 (deploy-from-main),
   and 8 (accept — gated behind a real prod deploy) do NOT run.
BLOCKED
exit 3

# ── Steps 6-8 are the INTENDED flow, gated behind the hard-stop above. They run only
# ── once deploy-from-main exists: delete the `exit 3` + this guard and uncomment.
# step "6 build-prod"  "$BUILDER"  chorus_build  "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"   # MUST build from MAIN
# step "7 deploy-prod" "$BUILDER"  chorus_deploy "$(printf '{"role":"%s","card_id":%s,"target":"canonical"}' "$BUILDER" "$CARD")"  # from MAIN
# step "8 accept"      "$ACCEPTER" werk-accept   "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"   # accepter via X-Chorus-Role, builder via role arg
# echo; echo "[done] all 8 steps green — #$CARD landed to prod via the live MCP flow."
