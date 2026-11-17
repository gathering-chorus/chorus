#!/usr/bin/env bats
# @test-type: integration — reads the live pulse API and Clearing v2 on this machine; run after the land
# @domain: streams
# @card: #4766
# @needs: machine
#
# AC5: after the land, the live Streams tab shows each logged-in role's recent tool calls from
# the generated route (pulse API /pulse/streams/events, read from the spine log), not :3470.
#
# Covers: directing/products/pulse/api/src/spine.rs
# Covers: directing/clearing-v2/src/sources/streams.ts

ROUTE="${STREAMS_ROUTE:-http://127.0.0.1:3392/pulse/streams/events}"
V2="${CLEARING_V2_STREAMS:-http://127.0.0.1:3472/clearing-v2/api/streams}"

# reads a route body on stdin; exit 0 when it holds a call from the last hour with a role.
RECENT='
import json, sys, datetime
rows = json.load(sys.stdin).get("data") or []
now = datetime.datetime.now(datetime.timezone.utc)
def recent(r):
    try:
        t = datetime.datetime.fromisoformat(str(r.get("streamTs", "")).replace("Z", "+00:00"))
    except ValueError:
        return False
    if t.tzinfo is None:
        t = t.replace(tzinfo=datetime.timezone.utc)
    return (now - t).total_seconds() < 3600
sys.exit(0 if any(r.get("streamRole") and recent(r) for r in rows) else 1)'

@test "AC5: the generated streams route serves recent tool calls with their roles" {
  body=$(curl -sf --max-time 10 "$ROUTE?limit=200")
  echo "$body" | python3 -c "$RECENT"
}

# The v2 lines must be the route's calls: a line that matches no route label came from somewhere
# else (the old Clearing), so a v2 still reading :3470 fails here even while it shows roles.
@test "AC5: Clearing v2's streams show the route's calls, by role" {
  route=$(curl -sf --max-time 10 "$ROUTE?limit=200")
  v2=$(curl -sf --max-time 10 "$V2")
  ROUTE_BODY="$route" python3 -c '
import json, os, sys
labels = {r.get("label") for r in json.loads(os.environ["ROUTE_BODY"]).get("data") or [] if r.get("label")}
d = json.load(sys.stdin)
lines = [l.get("text", "") for r in d.get("roles") or [] for l in r.get("lines") or []]
sys.exit(0 if d.get("ok") and lines and labels and any(any(lb in t for lb in labels) for t in lines) else 1)' <<< "$v2"
}

@test "AC5 NEGATIVE PROOF: an empty or stale route body fails the check" {
  run python3 -c "$RECENT" <<< '{"data":[]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< '{"data":[{"streamRole":"wren","streamTs":"2020-01-01T00:00:00Z"}]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}]}"
  [ "$status" -eq 0 ]
}
