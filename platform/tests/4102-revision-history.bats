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
drop = ("name","version","changedAt","changedIn","modified","created","ownedBy","label","iri")
# a read serves edge targets MINTED (chorus:value-stream-step-directing); the
# write mint adds the kind prefix itself (ADR-040 Rule 0), so a body echoing a
# read must hand back the bare name or the door refuses it double-prefixed.
def bare(v):
    if isinstance(v, list): return [bare(x) for x in v]
    if isinstance(v, str):
        n = v[len("chorus:"):] if v.startswith("chorus:") else v
        for kind in ("value-stream-step-","value-stream-","design-doc-","document-","domain-","service-","product-","role-"):
            if n.startswith(kind): return n[len(kind):]
        return n
    return v
keep = {k: bare(v) for k, v in r.items() if k not in drop and v not in ("", None, [])}
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
skip = {"version","changedAt","changedIn","modified","created","name","iri","type"}
diff = [k for k in set(snap) | set(now) if k not in skip and str(snap.get(k,"")) != str(now.get(k,""))]
print("changed:", sorted(diff))
# the field the write touched IS in the diff...
assert "gaps" in diff, diff
assert str(now.get("gaps")) == str(snap.get("gaps")) + " (bats-4102 touched)", (snap.get("gaps"), now.get("gaps"))
# ...and fields the write left alone are NOT (AC2: identical fields are not shown)
for same in ("promise", "vision", "structure", "audience"):
    assert same not in diff, (same, diff)' "$now"
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

@test "AC1/AC4: every page that renders a row carries the History fold, from one implementation" {
  fold="$ROOT/platform/api/public/athena/history-fold.js"
  grep -q "id=\"history\"" "$fold"
  grep -q "const diffOf" "$fold"
  # Jeff, 2026-09-04: "i can click on and view any revision" — the version opens,
  # not only its diff
  grep -q "view v\${r.v} as it was" "$fold"
  for f in product service; do
    grep -q "history-fold.js" "$ROOT/platform/api/public/athena/$f.html" || { echo "$f does not load the fold"; false; }
    grep -q "await historyFold(" "$ROOT/platform/api/public/athena/$f.html" || { echo "$f does not render the fold"; false; }
    # negative proof (#3734): no page keeps a private copy of the diff
    ! grep -q "const diffOf" "$ROOT/platform/api/public/athena/$f.html" || { echo "$f still has its own diff"; false; }
  done
}

@test "AC1: the model declares Revision, its shape and its claim on the products domain" {
  ttl="$ROOT/roles/silas/ontology/chorus.ttl"
  grep -q '^chorus:Revision a owl:Class' "$ttl"
  awk '/^chorus:RevisionShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:snapshot'
  awk '/^chorus:RevisionShape a sh:NodeShape/,/ \.$/' "$ttl" | grep -q 'sh:path chorus:ofRow'
  grep -q 'chorus:definesVocabulary chorus:Product, chorus:Revision' "$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
}

@test "AC4: a document replaced through the door keeps a Revision, and the document page carries the History fold" {
  live
  doc="$(curl -sf "$OWL_URL/documents" | python3 -c 'import sys,json; rows=json.load(sys.stdin)["data"]; r=[x for x in rows if x.get("docHref") and x.get("hasDomain")][0]; print(r["name"].replace("document-","",1))')"
  before="$(revisions_of "documents/$doc" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  # an entity read splits edges into links (#3635), so a body that echoes the row
  # has to take both halves or the shape refuses it for a missing hasDomain
  row="$(curl -sf "$OWL_URL/documents/$doc" | python3 -c 'import sys,json; e=json.load(sys.stdin); d=dict(e["data"]); d.update({k:v for k,v in (e.get("links") or {}).items() if k != "type"}); print(json.dumps(d))')"
  body="$(printf '%s' "$row" | python3 -c '
import sys, json
r = json.load(sys.stdin)
drop = ("name","version","changedAt","changedIn","modified","created","ownedBy","iri","creator","label")
def bare(v):
    if isinstance(v, list): return [bare(x) for x in v]
    if isinstance(v, str):
        n = v[len("chorus:"):] if v.startswith("chorus:") else v
        for kind in ("domain-","product-","role-","document-"):
            if n.startswith(kind): return n[len(kind):]
        return n
    return v
keep = {k: bare(v) for k, v in r.items() if k not in drop and v not in ("", None, [])}
keep["comment"] = (r.get("comment") or "") + " (bats-4102 touched)"
print(json.dumps(keep))')"
  run curl -s -o "$BATS_TEST_TMPDIR/put" -w '%{http_code}' -X PUT "$OWL_URL/documents/$doc" -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d "$body"
  [ "$output" = "200" ] || { cat "$BATS_TEST_TMPDIR/put"; false; }
  n="$(revisions_of "documents/$doc" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$n" -eq $((before + 1)) ] || { echo "document revisions before=$before after=$n"; false; }
  # a document has no page of its own (Jeff, 2026-09-04: "its just a fold on the
  # main page") — its history is a second fold on the product that owns it
  grep -q "historyFold('documents'" "$ROOT/platform/api/public/athena/product.html"
  grep -q 'id="dochistory"' "$ROOT/platform/api/public/athena/product.html"
  [ ! -f "$ROOT/platform/api/public/athena/document.html" ]
}

# --- one commit, one version (#4102) --------------------------------------
# A land posts a row more than once by design: a create, then a second pass
# restoring edges to rows that did not exist yet. Counted as two changes, a
# plain load left every row at v2 with a v1 revision nobody ever saw — history
# that records the loader, not a person. A write carrying the SAME commit as the
# row's current changedIn is the same change: same version, no revision kept.

product_body() {  # $1 = product name, $2 = marker text
  curl -sf "$OWL_URL/products" | python3 -c '
import sys, json
rows = json.load(sys.stdin)["data"]
r = [x for x in rows if x["name"] == sys.argv[1]][0]
drop = ("name","version","changedAt","changedIn","modified","created","ownedBy","label","iri")
def bare(v):
    if isinstance(v, list): return [bare(x) for x in v]
    if isinstance(v, str):
        n = v[len("chorus:"):] if v.startswith("chorus:") else v
        for kind in ("value-stream-step-","value-stream-","design-doc-","document-","domain-","service-","product-","role-"):
            if n.startswith(kind): return n[len(kind):]
        return n
    return v
keep = {k: bare(v) for k, v in r.items() if k not in drop and v not in ("", None, [])}
keep["gaps"] = (r.get("gaps") or "").split(" (bats-4102")[0] + " (bats-4102 " + sys.argv[2] + ")"
print(json.dumps(keep))' "$1" "$2"
}
product_version() { curl -sf "$OWL_URL/products" | python3 -c 'import sys,json; rows=json.load(sys.stdin)["data"]; print([x for x in rows if x["name"]==sys.argv[1]][0].get("version") or "0")' "$1"; }
put_product() {  # $1 = name, $2 = body, $3 = commit stamp ("" for a hand write)
  if [ -n "$3" ]; then
    curl -s -o /dev/null -w '%{http_code}' -X PUT "$OWL_URL/products/$1" -H "Authorization: Bearer $TOK" \
      -H 'Content-Type: application/json' -H "X-Landed-Commit: $3" -d "$2"
  else
    curl -s -o /dev/null -w '%{http_code}' -X PUT "$OWL_URL/products/$1" -H "Authorization: Bearer $TOK" \
      -H 'Content-Type: application/json' -d "$2"
  fi
}

@test "AC5: a second post from the same land is the same change — no new version, no revision" {
  live
  c="bats4102same$$"
  [ "$(put_product spine "$(product_body spine one)" "$c")" = "200" ]
  v1="$(product_version spine)"
  n1="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  # the second pass of the very same land, exactly as post_all sends it
  [ "$(put_product spine "$(product_body spine two)" "$c")" = "200" ]
  v2="$(product_version spine)"
  n2="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$v2" = "$v1" ] || { echo "version moved on the second post of one land: $v1 -> $v2"; false; }
  [ "$n2" -eq "$n1" ] || { echo "revisions kept on the second post of one land: $n1 -> $n2"; false; }
}

@test "AC5 negative proof (#3734): two DIFFERENT changes each keep their version — the rule swallows nothing" {
  live
  # a violation of what the rule is allowed to collapse: different commits.
  v0="$(product_version spine)"
  n0="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$(put_product spine "$(product_body spine cA)" "bats4102a$$")" = "200" ]
  [ "$(put_product spine "$(product_body spine cB)" "bats4102b$$")" = "200" ]
  n1="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$n1" -eq $((n0 + 2)) ] || { echo "two commits kept $((n1 - n0)) revisions, expected 2"; false; }
  # and a hand write carries no commit at all: it can never collapse into another
  [ "$(put_product spine "$(product_body spine hand1)" "")" = "200" ]
  [ "$(put_product spine "$(product_body spine hand2)" "")" = "200" ]
  n2="$(revisions_of products/spine | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))')"
  [ "$n2" -eq $((n1 + 2)) ] || { echo "two hand writes kept $((n2 - n1)) revisions, expected 2"; false; }
  v2="$(product_version spine)"
  [ "$v2" -eq $((v0 + 4)) ] || { echo "four changes moved the version $v0 -> $v2, expected +4"; false; }
}
