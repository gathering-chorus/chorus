#!/usr/bin/env bats
# @test-type: unit — reads the model files with arq and shacl; no store
# @domain: persistence
#
# #4422 — the persistence domain. A store row has exactly one parent (onDisk for a
# dataset, file or repo; inStore for a graph family) and exactly one role, with a
# role source unless it is the system of record. Rows are crawled, never deployed
# from a file (AC5). Model files read:
#   roles/silas/ontology/chorus.ttl
#   roles/wren/ontology/domains-wren-silas.ttl
# The store map (roles are authored there, never guessed):
#   platform/config/store-map.json
# Fixtures:
#   platform/tests/fixtures/persistence-4422/good.ttl
#   platform/tests/fixtures/persistence-4422/bad-two-parents.ttl
#   platform/tests/fixtures/persistence-4422/bad-no-parent.ttl
#   platform/tests/fixtures/persistence-4422/bad-copy-no-source.ttl
#   platform/tests/fixtures/persistence-4422/bad-record-with-source.ttl

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
DOMAINS="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
FIX="$ROOT/platform/tests/fixtures/persistence-4422"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#>'

ask() { # ask <query body> <files…> → prints true or false
  local q="$1"; shift
  local data=(); for f in "$@"; do data+=(--data "$f"); done
  arq "${data[@]}" --results=csv "$P ASK { $q }" 2>/dev/null | tail -1 | tr -d '\r'
}

setup_file() {
  # the PersistenceStore shape alone, so the fixtures are judged by it and nothing else
  SHAPE="$BATS_FILE_TMPDIR/shape.ttl"
  { grep '^@prefix' "$MODEL"
    awk '/^chorus:PersistenceStoreShape a sh:NodeShape/{p=1} p{print} p && /\. *$/ && !/^ *#/{exit}' "$MODEL"; } > "$SHAPE"
  export SHAPE
}

conforms() { shacl validate --shapes "$SHAPE" --data "$1" 2>&1 | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'; }

@test "the shape was found in the model" {
  grep -q 'sh:targetClass chorus:PersistenceStore' "$SHAPE"
}

@test "a graph family in its dataset, copied from git, conforms" {
  [ "$(conforms "$FIX/good.ttl")" = "true" ]
}

@test "NEGATIVE PROOF: a graph row with both onDisk and inStore is refused" {
  [ "$(conforms "$FIX/bad-two-parents.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a row with neither onDisk nor inStore is refused" {
  [ "$(conforms "$FIX/bad-no-parent.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a copy that names no source is refused" {
  [ "$(conforms "$FIX/bad-copy-no-source.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a system of record that names a source is refused" {
  [ "$(conforms "$FIX/bad-record-with-source.ttl")" = "false" ]
}

@test "persistence is the one domain that claims PersistenceStore, and it sits in the chain" {
  [ "$(ask 'chorus:persistence chorus:definesVocabulary chorus:PersistenceStore ; chorus:atStep ?s ; chorus:ownedBy ?o .
            chorus:PersistenceStore chorus:inHierarchy "chain" ; chorus:parentVia chorus:onDisk , chorus:inStore .
            FILTER NOT EXISTS { ?d chorus:definesVocabulary chorus:PersistenceStore FILTER(?d != chorus:persistence) }' "$MODEL" "$DOMAINS")" = "true" ]
}

no_seeded_stores() { # AC5: no model or seed file carries a PersistenceStore row
  [ "$(ask '?s a chorus:PersistenceStore' "$@")" = "false" ]
}

model_files() { ls "$ROOT"/roles/*/ontology/*.ttl "$ROOT"/designing/data/*.ttl 2>/dev/null; }

@test "AC5: no file in the model or seed set writes a PersistenceStore row" {
  run model_files; [ "${#lines[@]}" -gt 10 ]
  no_seeded_stores $(model_files)
}

@test "NEGATIVE PROOF: a seed file with a store row fails the AC5 check" {
  bad="$BATS_TEST_TMPDIR/seed.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:persistence-store-x a chorus:PersistenceStore .\n' > "$bad"
  run no_seeded_stores $(model_files) "$bad"
  [ "$status" -ne 0 ]
}

MAP="$ROOT/platform/config/store-map.json"

map_roles_are_whole() { # every store has one of the four roles, a source exactly when it is not the record,
                        # and every source or parent names a store in the map
  python3 -I - "$1" <<'PY'
import json, sys
stores = json.load(open(sys.argv[1]))["stores"]
names = {s["name"] for s in stores}
bad = []
for s in stores:
    role, src = s.get("role"), s.get("source")
    if role not in ("systemOfRecord", "copyOf", "derivedFrom", "cacheOf"): bad.append(f"{s['name']}: role {role!r}")
    if (role == "systemOfRecord") == (src is not None): bad.append(f"{s['name']}: role {role} with source {src!r}")
    if s.get("basis") not in ("measured", "inventory", "inferred"): bad.append(f"{s['name']}: no basis for its role")
    for k in ("source", "inStore"):
        if s.get(k) and s[k] not in names: bad.append(f"{s['name']}: {k} {s[k]!r} names no store")
print("\n".join(bad)); sys.exit(1 if bad else 0)
PY
}

@test "the store map: every store has one role, says how it is known, a source exactly when it copies, no dangling names" {
  run map_roles_are_whole "$MAP"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: a copy with no source in the map fails the check" {
  bad="$BATS_TEST_TMPDIR/map.json"
  python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); s=[x for x in d["stores"] if x.get("source")][0]; del s["source"]; json.dump(d,open(sys.argv[2],"w"))' "$MAP" "$bad"
  run map_roles_are_whole "$bad"
  [ "$status" -ne 0 ]
}

# The schedule (Wren 2026-10-11): infra before stores, every hour, and an infra
# finding never stops the stores crawl. Unit file:
#   platform/launchd/com.chorus.infra-stores-crawl.plist
UNIT="$ROOT/platform/launchd/com.chorus.infra-stores-crawl.plist"

infra_then_stores() {
  plutil -lint "$1" >/dev/null || return 1
  cmd=$(plutil -extract ProgramArguments.2 raw -o - "$1") || return 1
  every=$(plutil -extract StartInterval raw -o - "$1") || return 1
  [ "$every" -le 3600 ] || { echo "runs every ${every}s"; return 1; }
  # #4405 reopen 4 — each pass is a herald_run call (the one runner)
  case "$cmd" in
    *'"sensor":"infra"'*'; '*'"sensor":"stores"'*) return 0 ;;
    *) echo "not infra-then-stores with ';': $cmd"; return 1 ;;
  esac
}

@test "the crawl unit runs infra, then stores, hourly, and a finding in infra does not stop stores" {
  run infra_then_stores "$UNIT"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: stores before infra, or joined by &&, fails the check" {
  bad="$BATS_TEST_TMPDIR/swapped.plist"
  sed 's|"sensor":"infra"|"sensor":"SWAP"|; s|"sensor":"stores"|"sensor":"infra"|; s|"sensor":"SWAP"|"sensor":"stores"|' "$UNIT" > "$bad"
  run infra_then_stores "$bad"; [ "$status" -ne 0 ]
  sed 's|}'"'"'; |}'"'"' \&amp;\&amp; |g' "$UNIT" > "$bad"
  run infra_then_stores "$bad"; [ "$status" -ne 0 ]
}
