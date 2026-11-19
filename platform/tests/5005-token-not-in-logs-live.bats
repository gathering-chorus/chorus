#!/usr/bin/env bats
# @test-type: e2e — reads the live Clearing log through Loki and the token file's mtime
# @domain: security
# @card: #5005
# @owner: silas
# @needs: machine — the running Clearing on Library and its Loki stream
#
# #5005 AC2, proven after the land: the Clearing printed its remote-access
# token at every start ("[clearing] remote access token: <hex>"), into
# chorus-bridge.log and from there into Loki. After the land the start says
# only "remote access token is set"; Jeff then rotates the token and restarts.
#
# The check never reads the token. It reads only log lines and the file's
# mtime, so it cannot leak what it guards.
# NEGATIVE PROOF: case 2 matches the old line's exact shape; a start of the
# old build after the first notice turns it red (the 2026-11-16 lines in
# chorus-bridge.log are that shape).

load test_helper

LOKI="${LOKI_URL:-http://localhost:3102}"
# The Clearing runs as the account that owns the canonical checkout.
OWNER_HOME="${CLEARING_OWNER_HOME:-${CHORUS_ROOT%/CascadeProjects/chorus}}"
TOKEN_FILE="${CLEARING_TOKEN_FILE:-$OWNER_HOME/.chorus/bridge-auth-token}"
STREAM="{filename=\"$OWNER_HOME/Library/Logs/Chorus/chorus-bridge.log\"}"
# The notice is read from server.ts, its one home: if it is renamed or moved
# this file fails loudly here, never reports UNMEASURED on a stale string.
NOTICE=$(sed -n "/export function bridgeTokenNotice/,/^}/s/.*return '\\(.*\\)';/\\1/p" "$CHORUS_ROOT/directing/clearing/src/server.ts")

# loki_ts <filter> <start_ns> <first|last> → ns timestamp of the first/last match, or empty
loki_ts() {
  local end; end=$(python3 -c 'import time; print(int(time.time()*1e9))')
  curl -sf -m 20 -G "$LOKI/loki/api/v1/query_range" \
    --data-urlencode "query=$STREAM $1" \
    --data-urlencode "start=$2" --data-urlencode "end=$end" --data-urlencode "limit=5000" \
  | python3 -c '
import json, sys
ts = sorted(int(t) for s in json.load(sys.stdin)["data"]["result"] for t, _ in s["values"])
if ts: print(ts[0] if sys.argv[1] == "first" else ts[-1])
' "$3"
}

setup() {
  [ -n "$NOTICE" ] || { echo "bridgeTokenNotice() not found in directing/clearing/src/server.ts"; return 1; }
  curl -sf -m 5 "$LOKI/ready" >/dev/null || skip "UNMEASURED: Loki not reachable at $LOKI"
  # Loki keeps 30 days; look back 29.
  START=$(python3 -c 'import time; print(int((time.time()-29*86400)*1e9))')
  FIRST_NOTICE=$(loki_ts "|= \"$NOTICE\"" "$START" first)
}

@test "the landed Clearing has started and printed only the notice" {
  [ -n "$FIRST_NOTICE" ] || { echo "no '$NOTICE' line in Loki: the new build never started"; false; }
}

@test "no start since the new build printed a token value" {
  [ -n "$FIRST_NOTICE" ] || skip "UNMEASURED: the new build has not started"
  leak=$(loki_ts '|~ "\\[clearing\\] remote access token: [0-9a-f]{8}"' "$FIRST_NOTICE" last)
  [ -z "$leak" ] || { echo "token value logged at ns $leak, after the first notice"; false; }
}

@test "the token was rotated after the new build started, and Clearing restarted on it" {
  [ -n "$FIRST_NOTICE" ] || skip "UNMEASURED: the new build has not started"
  [ -e "$TOKEN_FILE" ] || skip "UNMEASURED: cannot see $TOKEN_FILE from this account"
  mtime_ns=$(( $(stat -f %m "$TOKEN_FILE") * 1000000000 ))
  [ "$mtime_ns" -gt "$FIRST_NOTICE" ] || { echo "token file unchanged since the new build started: not rotated"; false; }
  after=$(loki_ts "|= \"$NOTICE\"" "$mtime_ns" last)
  [ -n "$after" ] || { echo "no Clearing start since the rotation: the old token is still live"; false; }
}
