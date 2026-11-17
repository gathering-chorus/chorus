#!/usr/bin/env bats
# @test-type: integration — reads the live pulse API and Clearing v2 on this machine; run after the land
# @domain: streams
# @card: #4766
# #4986 (AC2): the live tab is one stream, every role interleaved, newest last
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

# reads Clearing v2's /api/streams on stdin (route body in ROUTE_BODY); exit 0 when its lines are
# the route's calls (a line matching no route label came from elsewhere, e.g. the old Clearing),
# more than one role appears, and the last line is one of the route's newest calls (newest last).
INTERLEAVED='
import json, os, sys
rows = json.loads(os.environ["ROUTE_BODY"]).get("data") or []
labels = [r.get("label") for r in rows if r.get("label")]
d = json.load(sys.stdin)
lines = d.get("lines") or []
texts = [l.get("text", "") for l in lines]
roles = {l.get("role") for l in lines if l.get("role")}
ok = d.get("ok") and texts and labels and any(any(lb in t for lb in labels) for t in texts)
ok = ok and len(roles) > 1 and any(lb in texts[-1] for lb in labels[:10])
sys.exit(0 if ok else 1)'

@test "AC5: the generated streams route serves recent tool calls with their roles" {
  body=$(curl -sf --max-time 10 "$ROUTE?limit=200")
  echo "$body" | python3 -c "$RECENT"
}

# The v2 lines must be the route's calls: a line that matches no route label came from somewhere
# else (the old Clearing), so a v2 still reading :3470 fails here even while it shows roles.
@test "AC5 + #4986 AC2: Clearing v2's stream is the route's calls, roles interleaved, newest last" {
  route=$(curl -sf --max-time 10 "$ROUTE?limit=200")
  v2=$(curl -sf --max-time 10 "$V2")
  ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< "$v2"
}

@test "#4986 NEGATIVE PROOF: one role only, or newest first, fails the interleave check" {
  route='{"data":[{"label":"new"},{"label":"old"}]}'
  run env ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"wren","text":"old"},{"role":"kade","text":"new"}]}'
  [ "$status" -eq 0 ]
  run env ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"wren","text":"old"},{"role":"wren","text":"new"}]}'
  [ "$status" -ne 0 ]
  run env ROUTE_BODY='{"data":[{"label":"new"}]}' python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"kade","text":"new"},{"role":"wren","text":"old"}]}'
  [ "$status" -ne 0 ]
}

@test "AC5 NEGATIVE PROOF: an empty or stale route body fails the check" {
  run python3 -c "$RECENT" <<< '{"data":[]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< '{"data":[{"streamRole":"wren","streamTs":"2020-01-01T00:00:00Z"}]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}]}"
  [ "$status" -eq 0 ]
}
