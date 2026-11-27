#!/usr/bin/env bats
# @test-type: unit — the rows plan against a fixture of the door (LAND_LIVE_JSON); dry run, no store, no network
# @domain: domains
# @card: #5045 AC6 · owner: wren (script by silas)
# #5045: land-5045-rows removes typed status and gaps from Domain rows, sets gap cards, fixes
# the design-doc links and names roles as the reader of principles — each row by its owner only.
# Offline: the live door is a fixture (LAND_LIVE_JSON); the runner is never called (dry run).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  S="$ROOT/platform/scripts/land-5045-rows"
  F="$BATS_TEST_TMPDIR/live.json"
  cat > "$F" <<'EOF'
{"domains": [
  {"name": "principles", "ownedBy": "principal-wren", "status": "building", "gaps": "values still link",
   "hasDesignDoc": "document-loom-subproduct-design"},
  {"name": "roles", "ownedBy": "principal-wren", "consumes": ["https://jeffbridwell.com/chorus#time"]},
  {"name": "identity", "ownedBy": "principal-silas", "status": "operating", "gaps": "one CSS",
   "hasDesignDoc": "document-service-design-security"},
  {"name": "tests", "ownedBy": "principal-kade", "status": "building"},
  {"name": "music", "ownedBy": "principal-kade"}
 ],
 "documents": [
  {"name": "gathering-docs-domain-loom", "ownedBy": "principal-jeff", "hasDomain": "principles"},
  {"name": "gathering-docs-loom-service-design", "ownedBy": "principal-jeff", "hasDomain": "principles"},
  {"name": "loom-subproduct-design", "ownedBy": "principal-wren", "hasDomain": "principles"},
  {"name": "service-design-identity", "ownedBy": "principal-silas", "hasDomain": "identity"}
 ]}
EOF
  export LAND_LIVE_JSON="$F"
}

@test "#5045 wren's plan clears principles' status, gaps and wrong design doc, and leaves roles' consumes alone" {
  run python3 "$S" --as wren
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # its one gap is now card 5047 (five value rows name removed principles), held by number
  echo "$output" | grep -q 'CHANGE domains/principles  {"gapCard": \["5047"\], "gaps": null, "hasDesignDoc": null, "status": null}'
  # the door refuses a patch of consumes (a repeated text field): the land of 11-27 06:17 stopped on it
  ! echo "$output" | grep -q 'domains/roles' || return 1
  ! echo "$output" | grep -q 'CHANGE documents/' || return 1  # a Document keeps its one domain (DocumentShape minCount 1)
}

@test "#5045 identity points at its own design, not security's" {
  run python3 "$S" --as silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'CHANGE domains/identity  {"gaps": null, "hasDesignDoc": "document-service-design-identity", "status": null}'
}

@test "#5045 NEGATIVE: another owner's rows are named, never written by the caller" {
  run python3 "$S" --as wren
  [ "$status" -eq 0 ]
  ! echo "$output" | grep -q '^CHANGE domains/tests' || return 1
  ! echo "$output" | grep -q '^CHANGE domains/identity' || return 1
  echo "$output" | grep -q '^OTHER  kade: 1 row(s), run as kade: domains/tests'
  ! echo "$output" | grep -q '^OTHER  jeff' || return 1
}

@test "#5045 NEGATIVE: a row with nothing typed is left alone, and a second run plans nothing" {
  # the after-land state: no status, no gaps, links already fixed, roles already consumes principles
  cat > "$LAND_LIVE_JSON" <<'EOF'
{"domains": [
  {"name": "principles", "ownedBy": "principal-wren", "gapCard": 5047},
  {"name": "roles", "ownedBy": "principal-wren", "consumes": ["https://jeffbridwell.com/chorus#time", "https://jeffbridwell.com/chorus#principles"]},
  {"name": "identity", "ownedBy": "principal-silas", "hasDesignDoc": "document-service-design-identity"}
 ],
 "documents": [
  {"name": "gathering-docs-domain-loom", "ownedBy": "principal-jeff"},
  {"name": "gathering-docs-loom-service-design", "ownedBy": "principal-jeff"},
  {"name": "loom-subproduct-design", "ownedBy": "principal-wren"}
 ]}
EOF
  for r in wren silas; do
    run python3 "$S" --as "$r"
    [ "$status" -eq 0 ] || { echo "$output"; return 1; }
    echo "$output" | grep -q 'door writes 0 add, 0 change, 0 delete'
  done
}

@test "#5045 NEGATIVE: no caller role refuses before any plan" {
  run env -u CHORUS_ROLE -u LAND_AS python3 "$S"
  [ "$status" -eq 2 ]
  echo "$output" | grep -q 'caller role unknown'
}

@test "#5045 NEGATIVE: a fixed edge whose row is gone fails loudly, never passes vacuously" {
  python3 - "$LAND_LIVE_JSON" <<'EOF'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
d["domains"] = [x for x in d["domains"] if x["name"] != "identity"]
json.dump(d, open(p, "w"))
EOF
  run python3 "$S" --as wren
  [ "$status" -eq 2 ]
  echo "$output" | grep -q 'domains/identity is not on the door'
}

@test "#5045 NEGATIVE: a row holding a different gap card is rewritten to the walked one" {
  python3 - "$LAND_LIVE_JSON" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
for x in d["domains"]:
    if x["name"] == "principles":
        x.pop("status"); x.pop("gaps"); x.pop("hasDesignDoc"); x["gapCard"] = [1]
json.dump(d, open(p, "w"))
PY
  run python3 "$S" --as wren
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'CHANGE domains/principles  {"gapCard": \["5047"\]}'
}
