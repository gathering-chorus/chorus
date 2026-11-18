#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door, read-only (negative proofs run anywhere)
# @domain: domains
# @card: #5002
#
# #5002 AC2 — every CodeKind and Language row is owned; real rows by the code domain's owner. Split from no-ownerless-rows-live.bats so each AC names its own check.
# Skips in a card's werk: it measures what landed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

owned_by_domain_owner() {
  python3 - "$1" "$2" <<'PY'
import json, sys
rows = json.loads(sys.argv[1]).get("data", [])
want = sys.argv[2]
local = lambda v: (v or "").rsplit("#", 1)[-1].removeprefix("chorus:")
probe = lambda r: (r.get("name") or "").startswith("zz-")
bad = [r.get("name") for r in rows if not local(r.get("ownedBy"))
       or (not probe(r) and local(r.get("ownedBy")) != want)]
if not rows:
    print("no rows read"); sys.exit(1)
print(f"{len(rows)} rows: " + (f"{len(bad)} ownerless or a real row not {want}'s: {', '.join(bad[:10])}" if bad else f"all owned; real rows all {want}'s"))
sys.exit(1 if bad else 0)
PY
}

rows_with_owners() { # $1 collection path: every row read by name, owner from links.ownedBy
  python3 - "$DOOR" "$1" <<'PY'
import json, sys, urllib.request
door, path = sys.argv[1], sys.argv[2]
get = lambda p: json.load(urllib.request.urlopen(door + p, timeout=20))
out = []
for r in get(path + "?limit=1000").get("data", []):
    e = get(path + "/" + r["name"])
    own = (e.get("links") or {}).get("ownedBy") or (e.get("data") or {}).get("ownedBy") or ""
    out.append({"name": r["name"], "ownedBy": own if isinstance(own, str) else json.dumps(own)})
print(json.dumps({"data": out}))
PY
}

@test "AC2 every CodeKind and Language row is owned by the code domain's owner" {
  in_werk && skip "after-land check: a card's werk has not landed"
  owner="$(curl -sf --max-time 20 "$DOOR/v1/domains/domains?limit=500" | python3 -c "import json,sys; print(next(((r.get('ownedBy') or '').rsplit('#',1)[-1] for r in json.load(sys.stdin)['data'] if r.get('name')=='code'), ''))")"
  [ -n "$owner" ] || { echo "the door names no owner for domain code"; return 1; }
  for c in kinds languages; do
    run owned_by_domain_owner "$(rows_with_owners "/v1/code/$c")" "$owner"
    echo "$c: $output"
    [ "$status" -eq 0 ] || return 1
  done
}

@test "AC2 NEGATIVE PROOF: an ownerless row, a real row owned by someone else, an ownerless probe, or no rows is red" {
  run owned_by_domain_owner '{"data":[{"name":"rust","ownedBy":"principal-kade"}]}' principal-kade
  [ "$status" -eq 0 ] || return 1
  run owned_by_domain_owner '{"data":[{"name":"rust","ownedBy":"principal-kade"},{"name":"toml"}]}' principal-kade
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"toml"* ]] || return 1
  run owned_by_domain_owner '{"data":[{"name":"rust","ownedBy":"principal-wren"}]}' principal-kade
  [ "$status" -eq 1 ] || return 1
  run owned_by_domain_owner '{"data":[]}' principal-kade
  [ "$status" -eq 1 ] || return 1
  run owned_by_domain_owner '{"data":[{"name":"rust","ownedBy":"principal-kade"},{"name":"zz-probe-1","ownedBy":"principal-quartet"}]}' principal-kade
  [ "$status" -eq 0 ] || { echo "a probe owned by its maker passes"; return 1; }
  run owned_by_domain_owner '{"data":[{"name":"rust","ownedBy":"principal-kade"},{"name":"zz-probe-1"}]}' principal-kade
  [ "$status" -eq 1 ] || { echo "an ownerless probe is red"; return 1; }
}
