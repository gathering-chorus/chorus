#!/usr/bin/env bash
# @test-type: integration — reads the live sweep log, the sessions door and the roles API
# @domain: identity
# @needs: machine — prod after the land
# @card: #4948 · owner: silas
# #4948 AC4 — in prod after the land: every agent role with an open pane session is listed
# by /api/chorus/context/roles however quiet it is, and the sweep sees live roles (kept > 0).
set -u
API="${CHORUS_API:-http://localhost:3340}"
# the sweep runs as Jeff, whose home holds its log; the land's prove runs as Jeff too
LOG="${SWEEP_LOG:-$HOME/Library/Logs/Gathering/session-sweep.log}"
[ -r "$LOG" ] || { echo "RED: cannot read $LOG"; exit 1; }
last=$(grep '^sweep:' "$LOG" | tail -1)
kept=$(printf '%s' "$last" | sed -nE 's/^sweep: ([0-9]+) live session\(s\) kept open.*/\1/p')
[ -n "$kept" ] || { echo "RED: the last sweep line is not the #4948 form: $last"; exit 1; }
[ "$kept" -gt 0 ] || { echo "RED: the sweep sees no live role: $last"; exit 1; }
open=$(curl -sf --max-time 10 "$API/owl/v1/identity/sessions?endedAt.absent=true&channel=pane&limit=5000") || { echo "RED: the sessions door did not answer"; exit 1; }
listed=$(curl -sf --max-time 10 "$API/api/chorus/context/roles") || { echo "RED: the roles API did not answer"; exit 1; }
missing=$(OPEN="$open" LISTED="$listed" python3 -c '
import json, os
o = json.loads(os.environ["OPEN"])["data"]
l = {r["name"] for r in json.loads(os.environ["LISTED"])["data"]["roles"]}
want = {r["ownedBy"].removeprefix("principal-") for r in o if r.get("sessionState") == "open" and r.get("ownedBy", "").startswith("principal-")}
print(" ".join(sorted(want - l)))
') || { echo "RED: could not compare the door and the roles API"; exit 1; }
[ -z "$missing" ] || { echo "RED: logged in but no tile: $missing"; exit 1; }
echo "GREEN: sweep kept $kept; every role with an open session has a tile"
