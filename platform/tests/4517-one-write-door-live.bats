#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door and store, read-only
# @domain: security
# @card: #4517
#
# #4517 AC7 (after test). After the land, measured on the live door with nothing written:
#   1 the door refuses before the body for every role, and agrees with the Permission rows
#     for every class (the AC2 invalid-body probe, each role from its own account);
#   2 every live role has exactly one open Session row, written by the door (its name is
#     the door's: <role>-<16 hex>, identity-login session_name; startedBy now names the
#     person present, #4412, so it is no marker of the writer);
#   3 the quartet test principal holds Append, and Write only on graphs whose classes carry
#     no ownedBy (moved from AC4, Jeff 11-17 23:50; narrowed by Jeff 11-18, #5002 AC6).
# #5002 AC6 (found live 11-18 09:49, Silas 09:50): a door-written class (writeAuthority
# "door", read from the shapes) must be refused whatever the graph grants.
# Skips in a card's werk: it measures what landed. Each verdict has a fixture negative proof.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
  PLAN="$ROOT/platform/tests/lib/scope-plan.py"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

# $1 open-sessions listing json, $2 live roles (space-separated): every role has exactly
# one open row, and that row's name is the door's.
sessions_verdict() {
  python3 - "$1" "$2" <<'PY'
import json, re, sys
rows = json.loads(sys.argv[1]).get("data", [])
local = lambda v: (v or "").rsplit("#", 1)[-1].rsplit("/", 1)[-1]
bad = []
for role in sys.argv[2].split():
    mine = [r for r in rows if local(r.get("ownedBy")) == f"principal-{role}"]
    if len(mine) != 1:
        bad.append(f"{role}: {len(mine)} open Session rows, want 1")
    elif not re.fullmatch(re.escape(role) + r"-[0-9a-f]{16}", mine[0].get("name") or ""):
        bad.append(f"{role}: open row {mine[0].get('name')} is not named by the door, so the door did not write it")
print("Session rows: " + ("; ".join(bad) or "one door-written open row per role"))
sys.exit(1 if bad else 0)
PY
}

# $1 Permission rows json, $2 class-graphs json, $3 owned json: the quartet holds Append,
# and Write only on graphs whose classes carry no ownedBy.
quartet_verdict() {
  local d; d="$(mktemp -d)"
  printf '%s' "$1" > "$d/p"; printf '%s' "$2" > "$d/g"; printf '%s' "$3" > "$d/o"
  python3 "$PLAN" quartet "$d/p" "$d/g" "$d/o"; local rc=$?; rm -rf "$d"; return $rc
}

live_roles() { # roles the door lists as Teammates with a live run — read from the door
  curl -sf --max-time 20 "$DOOR/v1/identity/sessions?endedAt.absent=true&channel=pane&limit=5000" \
    | python3 -c "import json,sys; print(' '.join(sorted({(r.get('ownedBy') or '').rsplit('#',1)[-1].removeprefix('principal-') for r in json.load(sys.stdin).get('data',[]) if r.get('ownedBy')})))"
}

@test "AC7 the live door refuses before the body and agrees with the Permission rows (this account's role)" {
  in_werk && skip "after-land check: a card's werk has not landed"
  role="${CHORUS_ROLE:-$(id -un | sed 's/^chorus-//')}"
  # #4915 — the nightly runs as jeffbridwell, a person with no role login: "no login for
  # jeffbridwell" read red every night from 11-19. Every role's door scope, door-written
  # classes included, is measured from that account by 4022-role-write-scope-live.bats.
  case "$(id -un)" in chorus-*) ;; *) [ -n "${CHORUS_ROLE:-}" ] || skip "this account has no role login; 4022-role-write-scope-live measures every role" ;; esac
  d="$BATS_TEST_TMPDIR"
  curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000" > "$d/perms.json"
  python3 "$PLAN" graphs "$DOOR" > "$d/graphs.json"; python3 "$PLAN" collections "$DOOR" > "$d/paths.txt"
  python3 "$PLAN" plan "$d/perms.json" "$d/graphs.json" "$role" > "$d/plan"
  python3 "$PLAN" door-written "$ROOT"/roles/silas/ontology/*.ttl > "$d/door"
  grep -qx Session "$d/door" || { echo "the shapes name no door-written Session class"; return 1; }
  tok="$("$ROOT/platform/scripts/chorus-identity-token" "$role" 2>/dev/null || true)"
  [ -n "$tok" ] || { echo "no login for $role from this account"; return 1; }
  : > "$d/out"
  while read -r _ cls; do
    cp_="$(awk -v c="$cls" '$1==c{print $2}' "$d/paths.txt")"
    code="$(curl -s -o "$d/body" -w '%{http_code}' --max-time 20 -X POST -H "Content-Type: application/json" \
      -H @<(printf 'Authorization: Bearer %s\n' "$tok") --data '{"zz-4517-invalid":1}' "$DOOR$cp_")"
    python3 "$PLAN" classify "$cls" "$code" "$d/body" >> "$d/out"
  done < "$d/plan"
  run python3 "$PLAN" verdict "$d/plan" "$d/out" "$d/door"
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

@test "AC7 the quartet test principal holds Append, and Write only where rows have no owner" {
  in_werk && skip "after-land check: a card's werk has not landed"
  run quartet_verdict "$(curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000")" \
    "$(python3 "$PLAN" graphs "$DOOR")" "$(python3 "$PLAN" owned "$DOOR")"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC7 NEGATIVE PROOF: two open rows, a role-written row, or a missing row are red; one door row passes" {
  door='{"name":"kade-0123456789abcdef","ownedBy":"principal-kade","startedBy":"principal-jeff"}'
  run sessions_verdict "{\"data\":[$door]}" "kade"
  [ "$status" -eq 0 ] || return 1
  run sessions_verdict "{\"data\":[$door,{\"name\":\"kade-fedcba9876543210\",\"ownedBy\":\"principal-kade\"}]}" "kade"
  [ "$status" -eq 1 ] || return 1
  # the old writer's name (11-18 live: kade-ez4pkngo-1a1c0ee3684) is not the door's
  run sessions_verdict '{"data":[{"name":"kade-ez4pkngo-1a1c0ee3684","ownedBy":"principal-kade","startedBy":"principal-jeff"}]}' "kade"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"not named by the door"* ]] || return 1
  run sessions_verdict '{"data":[]}' "kade"
  [ "$status" -eq 1 ] || return 1
}

@test "AC7 NEGATIVE PROOF: a quartet Write on an owned-class graph, or no Append, is red; Write on an ownerless graph passes" {
  A='{"agent":"principal-quartet","mode":"http://www.w3.org/ns/auth/acl#Append","accessTo":"urn:owned"}'
  WO='{"agent":"principal-quartet","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:owned"}'
  WF='{"agent":"principal-quartet","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:free"}'
  G='{"Card":"urn:owned","CodeKind":"urn:free"}'; O='{"Card":true,"CodeKind":false}'
  run quartet_verdict "{\"data\":[$A,$WF]}" "$G" "$O"
  [ "$status" -eq 0 ] || return 1
  run quartet_verdict "{\"data\":[$A,$WO]}" "$G" "$O"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Write on urn:owned: Card"* ]] || return 1
  run quartet_verdict "{\"data\":[$A,$WF]}" "$G" '{"Card":true}'
  [ "$status" -eq 1 ] || { echo "an unread class must never count as ownerless"; return 1; }
  run quartet_verdict "{\"data\":[$WF]}" "$G" "$O"
  [ "$status" -eq 1 ] || return 1
}

@test "AC7 NEGATIVE PROOF (#5002 AC6): a door-written class the door lets in is red; refused passes whatever the rows grant" {
  d="$BATS_TEST_TMPDIR"
  printf 'append Session\nallow Card\n' > "$d/plan"; echo Session > "$d/door"
  printf 'Session NOT-PERM written by the door\nCard PASS door allows\n' > "$d/ok"
  run python3 "$PLAN" verdict "$d/plan" "$d/ok" "$d/door"
  [ "$status" -eq 0 ] || return 1
  printf 'Session PASS door allows\nCard PASS door allows\n' > "$d/bad"
  run python3 "$PLAN" verdict "$d/plan" "$d/bad" "$d/door"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Session: door-written class"* ]] || return 1
  run python3 "$PLAN" verdict "$d/plan" "$d/ok"
  [ "$status" -eq 1 ] || { echo "without the door-written list the same refusal must stay red"; return 1; }
  printf 'chorus:FooShape\n    chorus:writeAuthority "door" ;\nchorus:BarShape\n    chorus:writeAuthority "deploy" ;\n' > "$d/s.ttl"
  [ "$(python3 "$PLAN" door-written "$d/s.ttl")" = "Foo" ] || return 1
}
