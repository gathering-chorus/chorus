#!/usr/bin/env bats
# @test-type: e2e — after the land: reads prod pulse, the door's runs and the live Clearing v2 people row
# @domain: roles
# @card: #4985
#
# AC2 (after test): a role logged in and quiet for 2 hours still has its tile in Clearing v2
# (#team), on the live page. Every role with a live run has a tile; at least one of them has
# been quiet 2 hours, else this pass cannot measure the AC and says so.

setup() {
  PULSE_URL="${PULSE_URL:-http://127.0.0.1:3475}"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
  V2_URL="${CLEARING_V2_URL:-http://127.0.0.1:3472}"
}

check() {  # $1 actor-state json, $2 runs json, $3 people html, $4 now (epoch s)
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import json, sys, re, datetime
st, runs, html, now = json.loads(sys.argv[1]), json.loads(sys.argv[2]), sys.argv[3], int(sys.argv[4])
live = {r["name"].split("-run-")[0] for r in runs.get("data", []) if "-run-" in r.get("name", "")}
tiles = set(re.findall(r'actor-tile-([a-z0-9-]+)\b', html)) - {"card", "info", "name"}
def ts(a):
    t = max((s.get("lastActionAt") or "" for s in a.get("sessions", [])), default="")
    try: return int(datetime.datetime.fromisoformat(t.replace("Z", "+00:00")).timestamp())
    except Exception: return 0
actors = {a.get("aboutPrincipal", "").removeprefix("principal-"): a for a in st.get("actors", [])}
quiet = []
for role in sorted(live - {"jeff"}):
    if role not in tiles:
        print(f"FAIL {role} has a live run and no tile"); sys.exit(1)
    last = ts(actors.get(role, {}))
    # a role with no recorded action is not "quiet 2 hours": it measures nothing
    if last and now - last >= 7200: quiet.append(role)
if not quiet:
    print("UNMEASURED no logged-in role has been quiet 2 hours this pass"); sys.exit(2)
print("ok quiet 2h with a tile: " + ", ".join(quiet))
PY
}

@test "every logged-in role has a tile, including one quiet 2 hours (live page)" {
  case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4985 AC2)";; esac
  st=$(curl -sf --max-time 10 "$PULSE_URL/api/actor-state")
  runs=$(curl -sf --max-time 10 "$DOOR/v1/identity/sessionruns?runEndedAt.absent=true&limit=5000")
  html=$(curl -sf --max-time 10 "$V2_URL/api/rooms/team/people")
  run check "$st" "$runs" "$html" "$(date +%s)"
  echo "$output"
  [ "$status" -ne 2 ] || skip "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a quiet logged-in role without a tile fails" {
  st='{"actors":[{"aboutPrincipal":"principal-kade","sessions":[{"lastActionAt":"2026-11-16T10:00:00Z"}]}]}'
  runs='{"data":[{"name":"kade-run-x"}]}'
  run check "$st" "$runs" '<a class="actor-tile actor-tile-wren">' "$(date +%s)"
  [ "$status" -eq 1 ]
}

@test "NEGATIVE PROOF: a role with a live run that pulse does not list, and no tile, fails" {
  run check '{"actors":[]}' '{"data":[{"name":"kade-run-x"}]}' '<a class="actor-tile actor-tile-wren">' "$(date +%s)"
  [ "$status" -eq 1 ]
}

@test "NEGATIVE PROOF: a role with no recorded action is not counted as quiet 2 hours" {
  run check '{"actors":[{"aboutPrincipal":"principal-kade","sessions":[]}]}' '{"data":[{"name":"kade-run-x"}]}' '<a class="actor-tile actor-tile-kade">' "$(date +%s)"
  [ "$status" -eq 2 ]
}
