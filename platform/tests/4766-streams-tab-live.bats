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

# reads a route body on stdin; exit 0 when it holds a call with a role from the hour before the
# newest tool call at the SOURCE (ANCHOR, epoch s; #4915 round 5: not now, so a quiet night is not
# red), and the route is not frozen: its newest row is within LAG of the source's newest call.
RECENT='
import json, os, sys, datetime
rows = json.load(sys.stdin).get("data") or []
now = datetime.datetime.fromtimestamp(float(os.environ["ANCHOR"]), datetime.timezone.utc)
lag = float(os.environ.get("LAG", "600"))
def ts(r):
    try:
        t = datetime.datetime.fromisoformat(str(r.get("streamTs", "")).replace("Z", "+00:00"))
    except ValueError:
        return None
    return t if t.tzinfo else t.replace(tzinfo=datetime.timezone.utc)
newest = max((t for t in map(ts, rows) if t), default=None)
if newest is None or (now - newest).total_seconds() > lag:
    print(f"frozen surface: the source has a call at {now.isoformat()}, the route newest {newest}")
    sys.exit(1)
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
# by exact label, unique in the route (marks stripped), and the matched route positions
# must only fall (route is newest first). v2 is read after the route, so calls newer than the route
# read match nothing and are skipped, never a race. When the route holds more than one role, v2
# must show more than one too (interleaved, not one role's column); one busy role alone is fine.
INTERLEAVED='
import json, os, sys
rows = json.loads(os.environ["ROUTE_BODY"]).get("data") or []
counts = {}
for r in rows:
    if r.get("label"):
        counts[r["label"]] = counts.get(r["label"], 0) + 1
# a label seen twice (the same command run again) cannot place a line in time: use unique ones only
at = {r["label"]: i for i, r in enumerate(rows) if r.get("label") and counts[r["label"]] == 1}
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
  line=$(python3 "$BATS_TEST_DIRNAME/lib/live_anchor.py" newest '{job="platform-chorus"} | json | event="agent.action"') \
    || skip "UNMEASURED: no tool call at the source (Loki) in 24h"
  body=$(curl -sf --max-time 10 "$ROUTE?limit=200")
  echo "$body" | ANCHOR="${line%%$'\t'*}" python3 -c "$RECENT"
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
  # the same command run twice (label "git status" at positions 0 and 2) places nothing; the rest still order
  dup='{"data":[{"label":"git status","streamRole":"kade"},{"label":"b","streamRole":"wren"},{"label":"git status","streamRole":"kade"},{"label":"a","streamRole":"kade"}]}'
  run env ROUTE_BODY="$dup" python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"kade","text":"a"},{"role":"kade","text":"git status"},{"role":"wren","text":"b"},{"role":"kade","text":"git status"}]}'
  [ "$status" -eq 0 ]
  # only one role active in the route: in order is enough
  run env ROUTE_BODY='{"data":[{"label":"y","streamRole":"wren"},{"label":"x","streamRole":"wren"}]}' python3 -c "$INTERLEAVED" <<< '{"ok":true,"lines":[{"role":"wren","text":"x"},{"role":"wren","text":"y"}]}'
  [ "$status" -eq 0 ]
}

@test "AC5 NEGATIVE PROOF: an empty, stale or frozen route body fails the check; a quiet night does not" {
  local now night
  now=$(date -u +%s); night=$((now - 5 * 3600))
  iso() { python3 -c "import sys,datetime; print(datetime.datetime.fromtimestamp(int(sys.argv[1]), datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'))" "$1"; }
  run env ANCHOR="$now" python3 -c "$RECENT" <<< '{"data":[]}'
  [ "$status" -ne 0 ]
  run env ANCHOR="$now" python3 -c "$RECENT" <<< '{"data":[{"streamRole":"wren","streamTs":"2020-01-01T00:00:00Z"}]}'
  [ "$status" -ne 0 ]
  # frozen: the source holds a call now, the route stopped 20 minutes ago
  run env ANCHOR="$now" python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(iso $((now - 1200)))\"}]}"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"frozen surface"* ]] || return 1
  run env ANCHOR="$now" python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(iso "$now")\"}]}"
  [ "$status" -eq 0 ]
  # a quiet night: the source's newest call was 5 hours ago and the route shows it, so green
  run env ANCHOR="$night" python3 -c "$RECENT" <<< "{\"data\":[{\"streamRole\":\"wren\",\"streamTs\":\"$(iso "$night")\"}]}"
  [ "$status" -eq 0 ]
}
