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
# #4915 round 5 (Wren 15:15) — anchored to the SOURCE, never to the route: ABBY_AT and REPLY_AT are
# the newest Abby call and reply in Loki (epoch s, empty when none in 24h). The route holds the spine
# log's tail, so a source event older than its oldest row cannot be asked of it (UNMEASURED, exit 2);
# one inside its span that the route lacks is a frozen or lossy surface (red).
ROUTE_HAS='
import json,os,sys,datetime
rows=json.load(sys.stdin).get("data") or []
def ts(r):
    try: d=datetime.datetime.fromisoformat(str(r.get("streamTs","")).replace("Z","+00:00"))
    except ValueError: return None
    return (d if d.tzinfo else d.replace(tzinfo=datetime.timezone.utc)).timestamp()
span=[t for t in map(ts,rows) if t is not None]
oldest=min(span) if span else None
def asked(var, has):
    at=os.environ.get(var,"")
    if not at or oldest is None or float(at) < oldest:
        return None                       # the route does not reach back to it: not measured
    return has
abby=asked("ABBY_AT", any(r.get("streamRole")=="abby-normal" and r.get("streamPhase")!="reply" for r in rows))
reply=asked("REPLY_AT", any(r.get("streamPhase")=="reply" for r in rows))
if abby is None and reply is None:
    print("UNMEASURED: neither an Abby call nor a reply at the source falls inside the route span"); sys.exit(2)
missing=[n for n,v in (("an Abby call",abby),("a reply",reply)) if v is False]
if missing:
    print("frozen or lossy surface: the source holds "+" and ".join(missing)+" inside the route span, the route does not"); sys.exit(1)
sys.exit(0)'

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

anchor_of() {  # newest matching event at the source (Loki), epoch seconds; empty when none in 24h
  local l; l=$(python3 "$BATS_TEST_DIRNAME/lib/live_anchor.py" newest "$1" 2>/dev/null) || return 0
  printf '%s' "${l%%$'\t'*}"
}

@test "AC5: the live streams route carries Abby's calls and at least one reply" {
  local abby reply body
  abby=$(anchor_of '{job="platform-chorus"} | json | event="agent.action" | role="abby-normal"')
  reply=$(anchor_of '{job="platform-chorus"} | json | event="reply.emitted"')
  body=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  run env ABBY_AT="$abby" REPLY_AT="$reply" python3 -c "$ROUTE_HAS" <<< "$body"
  [ "$status" -ne 2 ] || skip "$output"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC5: no role reads acting without rows of its own in the streams route" {
  route=$(curl -sf --max-time 10 "$ROUTE?limit=500")
  states=$(curl -sf --max-time 10 "$ACTORS")
  ROUTE_BODY="$route" python3 -c "$NONE_FALSELY_ACTING" <<< "$states"
}

@test "AC5 NEGATIVE PROOF: no Abby call, no reply, a role acting with no rows, or no actors at all each fail" {
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  # the route spans 10:00-11:00Z; the source's newest Abby call and reply are at 10:10Z, inside it
  local in=1795255800 old=1795240000
  t() { printf '"streamTs":"2026-11-21T%s:00Z"' "$1"; }
  span="{\"streamRole\":\"wren\",\"streamPhase\":\"done\",$(t 10:00)},{\"streamRole\":\"wren\",\"streamPhase\":\"done\",$(t 11:00)}"
  # NEGATIVE: the source has an Abby call inside the span and the route does not: frozen or lossy, red
  run env ABBY_AT="$in" REPLY_AT="$in" python3 -c "$ROUTE_HAS" <<< "{\"data\":[$span,{\"streamRole\":\"wren\",\"streamPhase\":\"reply\",$(t 10:30)}]}"
  [ "$status" -eq 1 ] || return 1
  # NEGATIVE: the source has a reply inside the span and the route does not: red
  run env ABBY_AT="$in" REPLY_AT="$in" python3 -c "$ROUTE_HAS" <<< "{\"data\":[$span,{\"streamRole\":\"abby-normal\",\"streamPhase\":\"done\",$(t 10:30)}]}"
  [ "$status" -eq 1 ] || return 1
  run env ABBY_AT="$in" REPLY_AT="$in" python3 -c "$ROUTE_HAS" <<< "{\"data\":[$span,{\"streamRole\":\"abby-normal\",\"streamPhase\":\"done\",$(t 10:30)},{\"streamRole\":\"wren\",\"streamPhase\":\"reply\",$(t 10:30)}]}"
  [ "$status" -eq 0 ] || return 1
  # a quiet Abby: her newest call is older than the route reaches, and no reply at all: UNMEASURED, never green
  run env ABBY_AT="$old" REPLY_AT="" python3 -c "$ROUTE_HAS" <<< "{\"data\":[$span]}"
  [ "$status" -eq 2 ] || return 1
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
