#!/usr/bin/env bats
# @test-type: integration — reads the live pulse API and pulse on this machine; run after the land
# @domain: streams
# @card: #4991
# @needs: machine
# #4991 (AC5): after the land, every role's calls and replies reach Streams, and Activity reads the routes
#
# Covers: designing/services/athena-gen/templates/spine_log.rs
# Covers: platform/pulse/src/actor-state.ts

ROUTE="${STREAMS_ROUTE:-http://127.0.0.1:3392/pulse/streams/events}"
ACTORS="${ACTOR_STATE:-http://127.0.0.1:3475/api/actor-state}"

@test "AC5: the live streams route carries Abby's calls and at least one reply" {
  body=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  echo "$body" | python3 -c '
import json,sys
rows=json.load(sys.stdin).get("data") or []
abby=any(r.get("streamRole")=="abby-normal" and r.get("streamPhase")!="reply" for r in rows)
reply=any(r.get("streamPhase")=="reply" for r in rows)
sys.exit(0 if abby and reply else 1)'
}

@test "AC5: a role with no rows in the streams or messages routes in 5 minutes does not read acting" {
  route=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  states=$(curl -sf --max-time 10 "$ACTORS")
  ROUTE_BODY="$route" python3 -c '
import json,os,sys,datetime
rows=json.loads(os.environ["ROUTE_BODY"]).get("data") or []
now=datetime.datetime.now(datetime.timezone.utc)
def recent(t):
    try: d=datetime.datetime.fromisoformat(str(t).replace("Z","+00:00"))
    except ValueError: return False
    return (now-d).total_seconds() < 300
active={r.get("streamRole") for r in rows if recent(r.get("streamTs"))}
items=json.load(sys.stdin).get("actors") or []
who=lambda s: str(s.get("aboutPrincipal","")).replace("principal-","")
bad=[who(s) for s in items if s.get("activity")=="acting" and who(s) not in active and who(s)!="jeff"]
# never vacuous: no actors read at all is a failure, not a pass
sys.exit(0 if items and not bad else 1)' <<< "$states"
}
