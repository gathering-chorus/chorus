#!/usr/bin/env bats
# @test-type: integration:api — signal:ui is fixture-data (greps the two pages for the History fold; the checks run against served rows)
load test_helper
#
# #4102 — the door keeps every prior version. Jeff, 2026-09-04: "at staples when we
# built athena we had a document level and field level revision history - this
# allowed us to run diffs on versions of docs to see what changed"; "having a
# revision history for the doc makes sense that is visible on the page"; "and
# retrievable by loom". What Jeff sees: replace a product through the API and a
# Revision row for the version it was appears, with its full data; the page's
# History fold lists it and shows what changed field by field; /revisions serves
# it to anyone (Loom). Negative proofs (#3734): a create keeps no revision; a
# direct write to /revisions is refused.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}
live() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  TOK="$("$ROOT/platform/scripts/chorus-identity-token" wren 2>/dev/null)"
  [ -n "$TOK" ] || skip "no identity token for wren"
}
revisions_of() { curl -sf --max-time 10 "$OWL_URL/revisions" | python3 -c 'import sys,json; d=json.load(sys.stdin); rows=d if isinstance(d,list) else d.get("data",[]); print(json.dumps([r for r in rows if r.get("ofRow")==sys.argv[1]]))' "$1"; }

@test "AC1: replacing a product through the door keeps the prior version as a Revision with its full data" {
  live
  before="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  row="$(curl -sf "$OWL_URL/products" | python3 -c 'import sys,json; rows=json.load(sys.stdin)["data"]; r=[x for x in rows if x["name"]=="spine"][0]; print(json.dumps(r))')"
  body="$(printf '%s' "$row" | python3 -c '
import sys, json
r = json.load(sys.stdin)
keep = {k: v for k, v in r.items() if k not in ("name","version","changedAt","changedIn","modified","created","ownedBy","label") and v not in ("", None, [])}
keep["gaps"] = (r.get("gaps") or "") + " (bats-4102 touched)"
print(json.dumps(keep))')"
  run curl -s -o "$BATS_TEST_TMPDIR/put" -w '%{http_code}' -X PUT "$OWL_URL/products/spine" -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d "$body"
  [ "$output" = "200" ] || { cat "$BATS_TEST_TMPDIR/put"; false; }
  after="$(revisions_of products/spine)"
  n="$(printf '%s' "$after" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$n" -eq $((before + 1)) ] || { echo "revisions before=$before after=$n"; false; }
  printf '%s' "$after" | python3 -c '
import sys, json
revs = json.load(sys.stdin); r = max(revs, key=lambda x: int(x.get("version") or 0))
snap = json.loads(r["snapshot"]); assert snap.get("promise"), "snapshot carries the full row"
assert r["ofRow"] == "products/spine" and str(r["version"]).isdigit()'
}

@test "AC2 (retrievable by Loom): the newest revision's snapshot differs from the row now in exactly the touched field" {
  live
  now="$(curl -sf "$OWL_URL/products" | python3 -c 'import sys,json; rows=json.load(sys.stdin)["data"]; print(json.dumps([x for x in rows if x["name"]=="spine"][0]))')"
  revisions_of products/spine | python3 -c '
import sys, json
revs = json.load(sys.stdin); r = max(revs, key=lambda x: int(x.get("version") or 0)); snap = json.loads(r["snapshot"]); now = json.loads(sys.argv[1])
skip = {"version","changedAt","changedIn","modified","created","name"}
diff = [k for k in set(snap) | set(now) if k not in skip and str(snap.get(k,"")) != str(now.get(k,""))]
print("changed:", diff); assert diff == ["gaps"], diff' "$now"
}

@test "AC3 negative proof (#3734): a create keeps no revision, and a direct write to /revisions is refused" {
  live
  before="$(curl -sf "$OWL_URL/revisions" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["data"]))')"
  curl -s -o /dev/null -X POST "$OWL_URL/documents" -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' \
    -d '{"name":"bats-4102-fresh","docTitle":"fresh","docHref":"/fresh.html","hasDomain":"products"}'
  after="$(curl -sf "$OWL_URL/revisions" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["data"]))')"
  [ "$after" -eq "$before" ] || { echo "a create made a revision: $before -> $after"; false; }
  run curl -s -o "$BATS_TEST_TMPDIR/out" -w '%{http_code}' -X POST "$OWL_URL/revisions" -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' \
    -d '{"name":"forged","ofRow":"products/spine","version":"99","snapshot":"{}","label":"x"}'
  [ "$output" = "422" ]
  grep -q 'kept by the door' "$BATS_TEST_TMPDIR/out"
}

@test "AC1/AC4: the product and service pages carry the History fold with field diffs" {
  for f in product service; do
    grep -q "id=\"history\"" "$ROOT/platform/api/public/athena/$f.html" || { echo "$f lacks History"; false; }
    grep -q "const diffOf" "$ROOT/platform/api/public/athena/$f.html"
  done
}

@test "AC1: the model declares Revision, its shape and its claim on the products domain" {
  ttl="$ROOT/roles/silas/ontology/chorus.ttl"
  grep -q '^chorus:Revision a owl:Class' "$ttl"
  awk '/^chorus:RevisionShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:snapshot'
  awk '/^chorus:RevisionShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:ofRow'
  grep -q 'chorus:definesVocabulary chorus:Product, chorus:Revision' "$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
}
