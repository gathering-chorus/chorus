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
# THE FLOW (per /tmp/werk-value-stream-v2.svg, Jeff's spec 2026-06-03):
#   — werk-demo boundary: prove in the role's slot, never touches main —
#   1 werk-commit   (rebase onto origin/main happens INSIDE commit, #3186)
#   2 werk-push
#   3 chorus_build  (build for the demo test)
#   4 chorus_deploy target=werk + chorus_env_up (stand up the running test instance)
#   — werk-acp boundary: land to prod; prod is a build of MAIN —
#   5 werk-merge    (#3175) resolve OPEN pr by HEAD oid, squash, content-verify
#   6 chorus_deploy target=canonical — SELF-BUILDS from canonical@origin/main (#3222)
#   7 werk-accept   X-Chorus-Role=accepter (jeff/wren); role-arg stays the builder
#
# #3222 closed the deploy-from-main gap: chorus_deploy target=canonical now builds the
# card's crate(s) from canonical@origin/main (werk-deploy → werk-build --target canonical
# --only, then chorus-deploy --target canonical per crate) — prod binaries are structurally
# a build of MAIN, not the werk. The old build-prod step is folded into step 6 (deploy
# owns the from-main build); the demo build at step 3 covers test-in-slot. All steps now
# run for real. demo-from-werk (3-4) = your slot; prod (6) = a build of main.
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

# Step 4.5 — werk-demo (#3116): the PROVING CEREMONY against the instance env-up
# just stood up. present → gate (delegated to the /demo skill's subagents) →
# feedback gather → review window → demo.verdict. The ACT (build/deploy/env-up,
# steps 3-4) is NOT here — demo only points at the running werk variant. Invoked
# as the binary (the /demo skill's path); a chorus_demo MCP wrapper is the
# consistency follow-on. Emits demo.verdict; werk-accept (step 8) gates on it.
echo "-- 4.5 demo  (as $BUILDER -> werk-demo binary) --------------------------------"
if DEPLOY_ROLE="$BUILDER" werk-demo "$CARD"; then echo "   [ok] 4.5 demo"
else echo "   [FAIL] 4.5 demo (werk-demo #$CARD) — chain stops here." >&2; exit 1; fi

# ═══ werk-acp boundary — land to prod; prod is a build of MAIN ═══
# Step 5 — werk-merge (#3175): the atomic MERGE verb, through the same MCP path as
# every other step. Resolves the OPEN PR for the current HEAD oid (NOT the branch
# name — retires the stale-PR false-green the interim `gh pr merge <branch>` caused,
# Wren + Kade 2026-06-03), squash-merges, and CONTENT-VERIFIES the merge landed.
step "5 merge"       "$BUILDER"  werk-merge    "$(printf '{"role":"%s","card_id":%s}' "$BUILDER" "$CARD")"

# #3234 — after merge, ff-sync canonical to origin/main so SCRIPT-only changes go LIVE
# with zero manual steps. Scripts are executables too: a script-only card (no service
# crate) merges but stays stale in canonical until a manual sync (bit #3222 + #3232) —
# the merged≠live lie for .sh as much as for binaries. deploy-prod (step 6) handles crate
# binaries; this makes canonical match origin/main immediately so everything else is live.
echo "-- 5.5 sync-canonical (scripts land live) --------------------------------"
if [ -n "${CHORUS_HOME:-}" ] \
   && git -C "$CHORUS_HOME" fetch -q origin main 2>/dev/null \
   && git -C "$CHORUS_HOME" merge --ff-only origin/main >/dev/null 2>&1; then
  echo "   [ok] 5.5 canonical ff-synced to origin/main"
  SYNC_OK=1
else
  echo "   [warn] 5.5 canonical NOT ff-synced (unset/diverged/dirty) — run 'chorus-werk-sync recover'; scripts stale until then" >&2
  SYNC_OK=0
fi

# ═══ Step 6 — deploy to PROD from MAIN (#3222 unblocked the hard-stop) ═══
# chorus_deploy target=canonical SELF-BUILDS from canonical@origin/main (werk-deploy →
# werk-build --target canonical --only <card crates>, then chorus-deploy per crate). prod
# binaries are structurally a build of main, not the werk — the merged≠live root is closed.
step "6 deploy-prod" "$BUILDER"  chorus_deploy "$(printf '{"role":"%s","card_id":%s,"target":"canonical"}' "$BUILDER" "$CARD")"  # builds + installs from MAIN

# #3234 — ACCEPT IS THE HUMAN'S HAND. werk-mcp STOPS here; it does NOT run werk-accept.
# A builder running a script must not auto-fire accept AS the accepter — the DEC-048
# human-accept gate can't be satisfied by an agent's script (house-always-wins). The work
# is landed to prod (built-of-main + live); the ACCEPT is the accepter's deliberate act.
echo
# #3234 — the "live" claim is CONDITIONAL on 5.5 succeeding. If canonical couldn't ff,
# a script-only change is merged-but-stale — claiming "live" would be the merged≠live lie
# in miniature (the exact class this card kills). Tell the truth either way.
if [ "${SYNC_OK:-0}" = "1" ]; then
  echo "[landed] #$CARD built-of-main, deployed + LIVE (steps 1-6 green). NOT yet accepted."
else
  echo "[merged, NOT yet live] #$CARD merged + deployed, but canonical did NOT ff-sync — script changes are STALE in prod. Run 'chorus-werk-sync recover' to make them live BEFORE accept." >&2
fi
echo "  Accept is yours (DEC-048) — judge the work, then run it yourself:"
echo "    DEPLOY_ROLE=$ACCEPTER werk-accept $CARD $BUILDER"
