#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door and store, read-only
# @domain: security
# @card: #4517
#
# #4517 AC7 (after test). After the land, measured on the live door with nothing written:
#   1 the door refuses before the body for every role, and agrees with the Permission rows
#     for every class (the AC2 invalid-body probe, each role from its own account);
#   2 every live role has exactly one open Session row, written by the door
#     (startedBy principal-door);
#   3 the quartet test principal holds Append, not Write (moved from AC4, Jeff 11-17 23:50).
# Skips in a card's werk: it measures what landed. Each verdict has a fixture negative proof.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
  PLAN="$ROOT/platform/tests/lib/scope-plan.py"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

# $1 open-sessions listing json, $2 live roles (space-separated): every role has exactly
# one open row, and that row's startedBy is the door.
sessions_verdict() {
  python3 - "$1" "$2" <<'PY'
import json, sys
rows = json.loads(sys.argv[1]).get("data", [])
local = lambda v: (v or "").rsplit("#", 1)[-1].rsplit("/", 1)[-1]
bad = []
for role in sys.argv[2].split():
    mine = [r for r in rows if local(r.get("ownedBy")) == f"principal-{role}"]
    if len(mine) != 1:
        bad.append(f"{role}: {len(mine)} open Session rows, want 1")
    elif local(mine[0].get("startedBy")) != "principal-door":
        bad.append(f"{role}: open row {mine[0].get('name')} started by {local(mine[0].get('startedBy')) or 'nobody'}, not the door")
print("Session rows: " + ("; ".join(bad) or "one door-written open row per role"))
sys.exit(1 if bad else 0)
PY
}

# $1 Permission rows json: the quartet principal holds Append on its graphs and Write on none.
quartet_verdict() {
  python3 - "$1" <<'PY'
import json, sys
rows = [r for r in json.loads(sys.argv[1]).get("data", []) if r.get("agent") == "principal-quartet"]
modes = {r.get("mode", "").rsplit("#", 1)[-1] for r in rows}
bad = []
if "Append" not in modes: bad.append("holds no Append row")
if "Write" in modes: bad.append("holds a Write row: it can change other owners' rows")
print("quartet: " + ("; ".join(bad) or f"Append on {len(rows)} graphs, no Write"))
sys.exit(1 if bad else 0)
PY
}

live_roles() { # roles the door lists as Teammates with a live run — read from the door
  curl -sf --max-time 20 "$DOOR/v1/identity/sessions?endedAt.absent=true&channel=pane&limit=5000" \
    | python3 -c "import json,sys; print(' '.join(sorted({(r.get('ownedBy') or '').rsplit('#',1)[-1].removeprefix('principal-') for r in json.load(sys.stdin).get('data',[]) if r.get('ownedBy')})))"
}

@test "AC7 the live door refuses before the body and agrees with the Permission rows (this account's role)" {
  in_werk && skip "after-land check: a card's werk has not landed"
  role="${CHORUS_ROLE:-$(id -un | sed 's/^chorus-//')}"
  d="$BATS_TEST_TMPDIR"
  curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000" > "$d/perms.json"
  python3 "$PLAN" graphs "$DOOR" > "$d/graphs.json"; python3 "$PLAN" collections "$DOOR" > "$d/paths.txt"
  python3 "$PLAN" plan "$d/perms.json" "$d/graphs.json" "$role" > "$d/plan"
  tok="$("$ROOT/platform/scripts/chorus-identity-token" "$role" 2>/dev/null || true)"
  [ -n "$tok" ] || { echo "no login for $role from this account"; return 1; }
  : > "$d/out"
  while read -r _ cls; do
    cp_="$(awk -v c="$cls" '$1==c{print $2}' "$d/paths.txt")"
    code="$(curl -s -o "$d/body" -w '%{http_code}' --max-time 20 -X POST -H "Content-Type: application/json" \
      -H @<(printf 'Authorization: Bearer %s\n' "$tok") --data '{"zz-4517-invalid":1}' "$DOOR$cp_")"
    python3 "$PLAN" classify "$cls" "$code" "$d/body" >> "$d/out"
  done < "$d/plan"
  run python3 "$PLAN" verdict "$d/plan" "$d/out"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC7 every live role has exactly one open Session row, written by the door" {
  in_werk && skip "after-land check: a card's werk has not landed"
  roles="$(live_roles)"
  [ -n "$roles" ] || { echo "no live role sessions to measure"; return 1; }
  listing="$(curl -sf --max-time 20 "$DOOR/v1/identity/sessions?endedAt.absent=true&limit=5000")"
  run sessions_verdict "$listing" "$roles"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC7 the quartet test principal holds Append, not Write" {
  in_werk && skip "after-land check: a card's werk has not landed"
  run quartet_verdict "$(curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000")"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC7 NEGATIVE PROOF: two open rows, a role-written row, or a missing row are red; one door row passes" {
  door='{"name":"kade-a","ownedBy":"principal-kade","startedBy":"principal-door"}'
  run sessions_verdict "{\"data\":[$door]}" "kade"
  [ "$status" -eq 0 ] || return 1
  run sessions_verdict "{\"data\":[$door,{\"name\":\"kade-b\",\"ownedBy\":\"principal-kade\",\"startedBy\":\"principal-door\"}]}" "kade"
  [ "$status" -eq 1 ] || return 1
  run sessions_verdict '{"data":[{"name":"kade-a","ownedBy":"principal-kade","startedBy":"principal-kade"}]}' "kade"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"not the door"* ]] || return 1
  run sessions_verdict '{"data":[]}' "kade"
  [ "$status" -eq 1 ] || return 1
}

@test "AC7 NEGATIVE PROOF: a quartet with Write, or with no Append, is red" {
  A='{"agent":"principal-quartet","mode":"http://www.w3.org/ns/auth/acl#Append","accessTo":"urn:g"}'
  W='{"agent":"principal-quartet","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:g"}'
  run quartet_verdict "{\"data\":[$A]}"
  [ "$status" -eq 0 ] || return 1
  run quartet_verdict "{\"data\":[$A,$W]}"
  [ "$status" -eq 1 ] || return 1
  run quartet_verdict '{"data":[]}'
  [ "$status" -eq 1 ] || return 1
}
