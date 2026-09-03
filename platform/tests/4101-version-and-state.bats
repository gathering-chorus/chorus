#!/usr/bin/env bats
# @test-type: integration:api — signal:ui is fixture-data (greps the two pages for the stamp markup; the checks run against served rows)
load test_helper
#
# #4101 — version is derived, state is declared. Jeff, 2026-09-03: "our docs dont
# show version and also show status", "these product docs have no version, does
# that matter, feel like it does, and what about state". What Jeff sees: every
# product, service and document row written through the door carries changedAt
# (UTC ISO) and changedIn (the land's commit), and every document carries one
# docState word from {draft, current, superseded, retired}; the product and
# service pages show the stamps. Negative proofs (#3734): a body that sets a stamp
# is refused; a docState outside the four words is refused by the shape.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}
live() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
}
rows() { curl -sf --max-time 10 "$OWL_URL/$1" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(json.dumps(d if isinstance(d,list) else d.get("data",[])))'; }

@test "AC1: every product and document served carries changedAt (UTC ISO) and changedIn (a commit, never 'unknown' after a land)" {
  live
  for k in products documents; do
    rows "$k" | python3 -c '
import sys, json, re
rows = json.load(sys.stdin); bad = []
for r in rows:
    at = str(r.get("changedAt") or ""); ci = str(r.get("changedIn") or "")
    if not re.match(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$", at) or not re.match(r"^[0-9a-f]{7,40}$", ci): bad.append((r.get("name"), at, ci))
print(sys.argv[1], "rows", len(rows), "bad", bad); sys.exit(1 if bad or not rows else 0)' "$k"
  done
}

@test "AC1: every document carries one docState word from the four" {
  live
  rows documents | python3 -c '
import sys, json
rows = json.load(sys.stdin); ok = {"draft","current","superseded","retired"}
bad = [(r.get("name"), r.get("docState")) for r in rows if r.get("docState") not in ok]
print("bad", bad); sys.exit(1 if bad or not rows else 0)'
}

@test "AC2 negative proof (#3734): a write body that sets changedIn is refused by the door" {
  live
  tok="$("$ROOT/platform/scripts/chorus-identity-token" wren 2>/dev/null)"
  [ -n "$tok" ] || skip "no identity token for wren"
  run curl -s -o "$BATS_TEST_TMPDIR/out" -w '%{http_code}' -X POST "$OWL_URL/documents" -H "Authorization: Bearer $tok" -H 'Content-Type: application/json' \
    -d '{"name":"bats-4101-stamp","docTitle":"x","docHref":"/x.html","changedIn":"deadbeef"}'
  [ "$output" = "422" ]
  grep -q 'stamped by the door' "$BATS_TEST_TMPDIR/out"
}

@test "AC3 negative proof (#3734): a docState outside the four words is refused by the shape" {
  live
  tok="$("$ROOT/platform/scripts/chorus-identity-token" wren 2>/dev/null)"
  [ -n "$tok" ] || skip "no identity token for wren"
  run curl -s -o "$BATS_TEST_TMPDIR/out" -w '%{http_code}' -X POST "$OWL_URL/documents" -H "Authorization: Bearer $tok" -H 'Content-Type: application/json' \
    -d '{"name":"bats-4101-state","docTitle":"x","docHref":"/x.html","docState":"maybe"}'
  [ "$output" = "422" ]
  grep -qi 'docState' "$BATS_TEST_TMPDIR/out"
}

@test "AC1: the product and service pages show the stamp" {
  grep -q "changed \${esc(String(e.changedAt).slice(0, 10))}" "$ROOT/platform/api/public/athena/product.html"
  grep -q "Changed (stamped by the door" "$ROOT/platform/api/public/athena/service.html"
}

@test "AC1: the model declares the stamps on Product, Service and Document, and docState on Document with the four words" {
  ttl="$ROOT/roles/silas/ontology/chorus.ttl"
  for shape in ProductShape ServiceShape DocumentShape; do
    awk "/^chorus:$shape a sh:NodeShape/,/ \\.\$/" "$ttl" | grep -q 'sh:path chorus:changedAt' || { echo "$shape lacks changedAt"; false; }
    awk "/^chorus:$shape a sh:NodeShape/,/ \\.\$/" "$ttl" | grep -q 'sh:path chorus:changedIn' || { echo "$shape lacks changedIn"; false; }
  done
  awk '/^chorus:DocumentShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:docState.*"draft" "current" "superseded" "retired"'
  ! awk '/^chorus:ProductShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:docState'
}
