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

# reads a streams route body on stdin; exit 0 when it holds a call by abby-normal and a reply
ROUTE_HAS='
import json,sys
rows=json.load(sys.stdin).get("data") or []
abby=any(r.get("streamRole")=="abby-normal" and r.get("streamPhase")!="reply" for r in rows)
reply=any(r.get("streamPhase")=="reply" for r in rows)
sys.exit(0 if abby and reply else 1)'

# reads pulse actor-state on stdin (route body in ROUTE_BODY); exit 0 when no principal but jeff
# reads acting without a row of its own in the last 5 minutes; an empty actors read fails
NONE_FALSELY_ACTING='
import json,os,sys,datetime
rows=json.loads(os.environ["ROUTE_BODY"]).get("data") or []
now=datetime.datetime.now(datetime.timezone.utc)
def recent(t,window=300):
    try: d=datetime.datetime.fromisoformat(str(t).replace("Z","+00:00"))
    except ValueError: return False
    if d.tzinfo is None: d=d.replace(tzinfo=datetime.timezone.utc)
    return (now-d).total_seconds() < window
# the same rule as pulse streamActivity: a reply is no work; a running call is acting now
active={r.get("streamRole") for r in rows if r.get("streamPhase")!="reply" and ((r.get("streamPhase")=="running" and recent(r.get("streamTs"),1800)) or recent(r.get("streamTs")))}
items=json.load(sys.stdin).get("actors") or []
who=lambda s: str(s.get("aboutPrincipal","")).replace("principal-","")
bad=[who(s) for s in items if s.get("activity")=="acting" and who(s) not in active and who(s)!="jeff"]
sys.exit(0 if items and not bad else 1)'

@test "AC5: the live streams route carries Abby's calls and at least one reply" {
  body=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  python3 -c "$ROUTE_HAS" <<< "$body"
}

@test "AC5: no role reads acting without rows of its own in the streams route" {
  route=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  states=$(curl -sf --max-time 10 "$ACTORS")
  ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< "$states"
}

@test "AC5 NEGATIVE PROOF: no Abby call, no reply, a role acting with no rows, or no actors at all each fail" {
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  run python3 -c "$ROUTE_HAS" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamPhase\":\"done\"},{\"streamRole\":\"wren\",\"streamPhase\":\"reply\"}]}"
  [ "$status" -ne 0 ] || return 1
  run python3 -c "$ROUTE_HAS" <<< "{\"data\":[{\"streamRole\":\"abby-normal\",\"streamPhase\":\"done\"}]}"
  [ "$status" -ne 0 ] || return 1
  run python3 -c "$ROUTE_HAS" <<< "{\"data\":[{\"streamRole\":\"abby-normal\",\"streamPhase\":\"done\"},{\"streamRole\":\"wren\",\"streamPhase\":\"reply\"}]}"
  [ "$status" -eq 0 ] || return 1
  route="{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$now\"}]}"
  run env ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< '{"actors":[{"aboutPrincipal":"principal-kade","activity":"acting"}]}'
  [ "$status" -ne 0 ] || return 1
  run env ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< '{"actors":[]}'
  [ "$status" -ne 0 ] || return 1
  run env ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< '{"actors":[{"aboutPrincipal":"principal-wren","activity":"acting"},{"aboutPrincipal":"principal-kade","activity":"idle"}]}'
  [ "$status" -eq 0 ] || return 1
  # jeff has no stream rows of his own (he types, he does not call tools): his acting is never flagged
  run env ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< '{"actors":[{"aboutPrincipal":"principal-jeff","activity":"acting"}]}'
  [ "$status" -eq 0 ] || return 1
}
