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

# reads Clearing v2's /api/streams on stdin (route body in ROUTE_BODY, fetched FIRST); exit 0 when
# v2's lines are the route's calls in time order, newest last: each line is matched to a route row
# by exact label (with the running/failed/stopped marks stripped), and the matched route positions
# must only fall (route is newest first). v2 is read after the route, so calls newer than the route
# read match nothing and are skipped, never a race. When the route holds more than one role, v2
# must show more than one too (interleaved, not one role's column); one busy role alone is fine.
INTERLEAVED='
import json, os, sys
rows = json.loads(os.environ["ROUTE_BODY"]).get("data") or []
at = {}
for i, r in enumerate(rows):
    if r.get("label") and r["label"] not in at:
        at[r["label"]] = i
def bare(t):
    t = t[2:] if t.startswith("\u23f3 ") else t
    for end in (" \u2717", " (stopped)"):
        if t.endswith(end):
            t = t[: -len(end)]
    return t
d = json.load(sys.stdin)
lines = d.get("lines") or []
pos = [at[bare(l.get("text", ""))] for l in lines if bare(l.get("text", "")) in at]
in_order = len(pos) >= 2 and all(a >= b for a, b in zip(pos, pos[1:]))
route_roles = {r.get("streamRole") for r in rows if r.get("streamRole")}
v2_roles = {l.get("role") for l in lines if l.get("role")}
roles_ok = len(route_roles) < 2 or len(v2_roles) > 1
sys.exit(0 if d.get("ok") and in_order and roles_ok else 1)'

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

@test "#4986 NEGATIVE PROOF: newest-first or one-role-only fails; one busy role alone passes" {
  route='{"data":[{"label":"c","streamRole":"kade"},{"label":"b","streamRole":"wren"},{"label":"a","streamRole":"kade"}]}'
  run env ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"kade","text":"a"},{"role":"wren","text":"b"},{"role":"kade","text":"\u23f3 c"}]}'
  [ "$status" -eq 0 ]
  # newest first
  run env ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"kade","text":"c"},{"role":"wren","text":"b"},{"role":"kade","text":"a"}]}'
  [ "$status" -ne 0 ]
  # grouped by role: kade a, kade c, then wren b
  run env ROUTE_BODY="$route" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"kade","text":"a"},{"role":"kade","text":"c"},{"role":"wren","text":"b"}]}'
  [ "$status" -ne 0 ]
  # only one role active in the route: in order is enough
  run env ROUTE_BODY='{"data":[{"label":"y","streamRole":"wren"},{"label":"x","streamRole":"wren"}]}' python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"wren","text":"x"},{"role":"wren","text":"y"}]}'
  [ "$status" -eq 0 ]
}

@test "AC5 NEGATIVE PROOF: an empty or stale route body fails the check" {
  run python3 -c "$RECENT" <<< '{"data":[]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< '{"data":[{"streamRole":"wren","streamTs":"2020-01-01T00:00:00Z"}]}'
  [ "$status" -ne 0 ]
  run python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}]}"
  [ "$status" -eq 0 ]
}
