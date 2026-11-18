#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door, read-only
# @domain: domains
# @card: #5002
#
# #5002 AC2 (after test). Once AC1 has landed, CodeKind and Language declare ownedBy, so the
# door serves each row's owner. Every row in both classes has one, and each owner is the code
# domain's owner (Jeff 09-17: the domain owner owns its rows), read from the door, never a list.
# Nothing is written. Skips in a card's werk: it measures what landed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

# $1 rows json ({"data":[{name, ownedBy}]}), $2 domain owner (principal-x): every row has an
# owner, and every real row is the domain owner's. A zz-probe row is owned by the principal
# whose probe made it (the quartet's Append creates, Kade 10:18), so it needs an owner only. The list view leaves ownedBy out (Kade 10:17), so rows_with_owners reads each
# row and takes links.ownedBy, as the door serves it.
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

# #5002 AC3 (Silas 11:01: a ratchet, sampled). $1 known-issues tsv (the file athena-validate compiles
# in), $2 reads ("Class<TAB>name<TAB>code" lines): every row the door cannot read by name falls in a
# named group (check row-not-found-at-the-door, class, rule); one outside every group is red.
unreadable_named() {
  python3 - "$1" "$2" <<'PY'
import sys
groups = [l.split("\t")[:3] for l in open(sys.argv[1]) if l.strip() and not l.startswith("#")]
def fits(rule, name):
    k, _, x = rule.partition(":")
    return {"prefix": name.startswith(x), "contains": x in name, "exact": name == x}.get(k, False) if x else \
        (rule == "any" or (rule == "uppercase-first" and name[:1].isupper()))
outside = []
for line in open(sys.argv[2]):
    cls, name, code = line.rstrip("\n").split("\t")
    if code == "200":
        continue
    if not any(c == "row-not-found-at-the-door" and k == cls and fits(r, name) for c, k, r in groups):
        outside.append(f"{cls}/{name} ({code})")
print("unreadable outside a named group: " + (", ".join(outside[:10]) or "none"))
sys.exit(1 if outside else 0)
PY
}

sample_reads() { # 5 listed rows per collection, read by name: Class<TAB>name<TAB>code
  python3 - "$DOOR" <<'PY'
import json, sys, urllib.request, urllib.parse
door = sys.argv[1]
for p in json.load(urllib.request.urlopen(door + "/", timeout=20))["primitives"]:
    try:
        names = [r["name"] for r in json.load(urllib.request.urlopen(door + p["collection"] + "?limit=5", timeout=40)).get("data", [])]
    except Exception:
        continue
    for n in names:
        try:
            urllib.request.urlopen(door + p["collection"] + "/" + urllib.parse.quote(n, safe=""), timeout=20); code = "200"
        except urllib.error.HTTPError as e:
            code = str(e.code)
        except Exception:
            code = "unanswered"
        print(f"{p['kind']}\t{n}\t{code}")
PY
}

@test "AC3 every door-listed row reads by name or sits in a named group (sampled)" {
  in_werk && skip "after-land check: a card's werk has not landed"
  sample_reads > "$BATS_TEST_TMPDIR/reads"
  [ -s "$BATS_TEST_TMPDIR/reads" ] || { echo "the door listed nothing"; return 1; }
  run unreadable_named "$ROOT/designing/domains/domains/validator-known-issues.tsv" "$BATS_TEST_TMPDIR/reads"
  echo "$(wc -l < "$BATS_TEST_TMPDIR/reads") reads; $output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC3 NEGATIVE PROOF: an unreadable row outside every group is red; a grouped one and a readable one pass" {
  g="$ROOT/designing/domains/domains/validator-known-issues.tsv"
  printf 'Appointment\tappt-x\t404\nCard\tcard-1\t200\n' > "$BATS_TEST_TMPDIR/ok"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/ok"
  [ "$status" -eq 0 ] || return 1
  printf 'Appointment\tappointment-new\t404\n' > "$BATS_TEST_TMPDIR/bad"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/bad"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Appointment/appointment-new"* ]] || return 1
  printf 'Card\tcard-1\tunanswered\n' > "$BATS_TEST_TMPDIR/un"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/un"
  [ "$status" -eq 1 ] || { echo "an unanswered read is never counted as readable"; return 1; }
}

# #5002 AC4 — $1 listed rows ("Class<TAB>name<TAB>servedFrom" lines): no zz-probe row is left
# outside the security graph (Silas takes those 5 himself, 10:33).
probes_left() {
  python3 - "$1" <<'PY'
import sys
left = [f"{c}/{n}" for c, n, g in (l.rstrip("\n").split("\t") for l in open(sys.argv[1]) if l.strip())
        if n.startswith("zz-probe-") and g != "urn:chorus:domains:security"]
print(f"zz-probe rows left: {len(left)}" + (": " + ", ".join(left[:10]) if left else ""))
sys.exit(1 if left else 0)
PY
}

listed_names() { # every listed row name per collection, Version aside (its probe rows go by ofRow, AC4's ledger line)
  python3 - "$DOOR" <<'PY'
import json, sys, urllib.request
door = sys.argv[1]
for p in json.load(urllib.request.urlopen(door + "/", timeout=20))["primitives"]:
    if p["kind"] == "Version":
        continue
    cur = p["collection"] + "?limit=1000"
    while cur:
        d = json.load(urllib.request.urlopen(door + cur, timeout=60))
        for r in d.get("data", []):
            print(f"{p['kind']}\t{r.get('name','')}\t{d.get('servedFrom','')}")
        cur = (d.get("links") or {}).get("next")
PY
}

# #5002 AC5 — $1 Permission rows json: the quartet holds Append and no Write row at all.
quartet_append_only() {
  python3 - "$1" <<'PY'
import json, sys
rows = [r for r in json.loads(sys.argv[1]).get("data", []) if (r.get("agent") or "").endswith("principal-quartet")]
w = [r.get("name") for r in rows if (r.get("mode") or "").endswith("Write")]
a = [r for r in rows if (r.get("mode") or "").endswith("Append")]
print(f"quartet: {len(a)} Append, {len(w)} Write" + (": " + ", ".join(w) if w else ""))
sys.exit(1 if w or not a else 0)
PY
}

@test "AC4 no zz-probe row is left outside the security graph" {
  in_werk && skip "after-land check: a card's werk has not landed"
  listed_names > "$BATS_TEST_TMPDIR/names"
  [ -s "$BATS_TEST_TMPDIR/names" ] || { echo "the door listed nothing"; return 1; }
  run probes_left "$BATS_TEST_TMPDIR/names"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC5 the quartet test principal holds Append and no Write row" {
  in_werk && skip "after-land check: a card's werk has not landed"
  run quartet_append_only "$(curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000")"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC4/AC5 NEGATIVE PROOF: a probe row present is red (security's are not), a quartet Write row is red, no Append is red" {
  printf 'Language\trust\turn:chorus:domains:code\nPrincipal\tzz-probe-x\turn:chorus:domains:security\n' > "$BATS_TEST_TMPDIR/clean"
  run probes_left "$BATS_TEST_TMPDIR/clean"
  [ "$status" -eq 0 ] || return 1
  printf 'Language\tzz-probe-1-language\turn:chorus:domains:code\n' > "$BATS_TEST_TMPDIR/probe"
  run probes_left "$BATS_TEST_TMPDIR/probe"
  [ "$status" -eq 1 ] || return 1
  A='{"name":"q-a","agent":"chorus:principal-quartet","mode":"Append"}'
  W='{"name":"quartet-write-values","agent":"chorus:principal-quartet","mode":"Write"}'
  run quartet_append_only "{\"data\":[$A]}"
  [ "$status" -eq 0 ] || return 1
  run quartet_append_only "{\"data\":[$A,$W]}"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"quartet-write-values"* ]] || return 1
  run quartet_append_only '{"data":[]}'
  [ "$status" -eq 1 ] || return 1
}
