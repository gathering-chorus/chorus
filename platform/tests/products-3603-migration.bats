#!/usr/bin/env bats
# @test-type: integration:api
# #3603 — proves the V1 product-layer retirement DONE-STATE against the live graph.
# RED until the migration is applied; GREEN is the definition of done.
# Target: SubProduct gone; products are typed chorus:Product children on the
# committed product-* IRI convention (designing/data/product-instances.ttl), with
# legacy hubs chorusProduct / borgProduct / gathering kept (IRI convergence = #1772).

NS="https://jeffbridwell.com/chorus#"
EP="http://localhost:3030/pods/sparql"

ask() { # $1 = WHERE body -> prints "True"/"False"
  curl -s --max-time 10 "$EP" --data-urlencode "query=PREFIX chorus: <$NS> ASK { GRAPH ?g { $1 } }" \
    -H "Accept: application/sparql-results+json" | python3 -c "import sys,json;print(json.load(sys.stdin)['boolean'])"
}
count() { # $1 = WHERE body -> prints integer
  curl -s --max-time 10 "$EP" --data-urlencode "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH ?g { $1 } }" \
    -H "Accept: application/sparql-results+json" | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])"
}

@test "no chorus:SubProduct instances remain" {
  [ "$(count '?s a chorus:SubProduct')" -eq 0 ]
}

# #4187 — chorusProduct and borgProduct were TWO names for products that already
# existed as chorus:chorus and chorus:borg, and the store held both halves with
# different predicates on each. They were unioned into the short names and
# retired on 2026-09-18 (model-retirements.jsonl), so asserting chorusProduct
# still exists asserts the duplicate we deliberately removed. The check that
# earns its place now is the opposite one: the merge target is typed, and the
# retired alias is gone.
@test "the chorus product is chorus:chorus, and the chorusProduct alias is retired" {
  [ "$(ask 'chorus:chorus a chorus:Product')" = "True" ]
  [ "$(ask 'chorus:chorusProduct ?p ?o')" = "False" ]
  [ "$(ask 'chorus:borgProduct ?p ?o')" = "False" ]
}

# #3915 — the `product-<slug>` IRIs this test asserted no longer exist: the
# products were re-minted on the bare slug (chorus:loom, chorus:werk …) and
# /products serves exactly 8 that way. The migration this file guards DID
# happen; the file kept checking the pre-mint names and reported the model as
# broken every night. Re-pointed at the served names, and the count is pinned
# so a SILENT product disappearing is still caught.
@test "the chorus child products are typed chorus:Product and partOf the chorus product" {
  for p in loom athena werk clearing convergence borg; do
    [ "$(ask "chorus:$p a chorus:Product")" = "True" ]
  done
}

@test "pulse and spine are both typed products" {
  # #3915 asserted spine was a DOMAIN, not a product. Wren minted the spine
  # Product on 2026-09-01 (#4045) — a deliberate model change, so the old
  # assertion is stale, not a regression. Both are products now.
  [ "$(ask 'chorus:pulse a chorus:Product')" = "True" ]
  [ "$(ask 'chorus:spine a chorus:Product')" = "True" ]
}

# #4265 — the named set is gone, and so is the fixture beside it. Jeff,
# 2026-09-21: "if a test tests data instance i dont think it belongs here like
# counts or specific names or values". This case held a literal list of ten
# product names and went red today because `gathering` finally passed the shape
# and started being served — reality improved and the test called it a break.
# Its own comment records the same list being edited on 09-18 and again on
# 09-19. A guard that needs editing whenever the data changes is a copy of the
# data, not a guard.
#
# What survives is what the collection must always do: serve typed rows that
# each carry a name, and never serve a name we retired. The retirement check is
# the case below and is unchanged.
@test "#4265: every served product is a named row (no anonymous or empty rows)" {
  body="$(curl -sf --max-time 10 http://localhost:3360/products)" || skip "UNMEASURABLE: owl-api not answering"
  run python3 -c '
import json,sys
d = json.loads(sys.argv[1]).get("data", [])
if not d:
    print("served nothing at all"); sys.exit(1)
bad = [x for x in d if not x.get("name")]
if bad:
    print("rows with no name: %d of %d" % (len(bad), len(d))); sys.exit(1)
print("ok %d named rows" % len(d))
' "$body"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

# NEGATIVE PROOF (#3734) — the check above must reject the states it names.
# An empty collection and a row without a name each have to fail it, or it is
# green for every payload including a broken one.
@test "NEGATIVE PROOF: the named-row check rejects an empty set and an unnamed row" {
  run python3 -c '
import json,sys
d = json.loads(sys.argv[1]).get("data", [])
if not d: print("served nothing at all"); sys.exit(1)
bad = [x for x in d if not x.get("name")]
if bad: print("rows with no name"); sys.exit(1)
print("ok")
' '{"data": []}'
  [ "$status" -ne 0 ]

  run python3 -c '
import json,sys
d = json.loads(sys.argv[1]).get("data", [])
if not d: print("served nothing at all"); sys.exit(1)
bad = [x for x in d if not x.get("name")]
if bad: print("rows with no name"); sys.exit(1)
print("ok")
' '{"data": [{"name": "athena"}, {"label": "no name here"}]}'
  [ "$status" -ne 0 ]
}

@test "quality-product and the product-borg dup are retired (gone)" {
  [ "$(ask 'chorus:quality-product ?p ?o')" = "False" ]
  [ "$(ask 'chorus:product-borg ?p ?o')" = "False" ]
}

@test "no subject points hasDomain at anything while itself untyped as Product" {
  # #3915 — two findings here, kept separate on purpose:
  #  (a) DOCUMENTS legitimately carry chorus:hasDomain (chorus:hasDomain's
  #      declared domain IS chorus:Document) — excluded below; the rule was
  #      never about them.
  #  (b) the REAL drift this assert has been catching correctly for weeks: 7
  #      stale `product-*` subjects still carry hasDomain/consumes/atStep while
  #      the served products live on bare slugs. That is #3916 (mine), not a
  #      test defect — this stays RED until the retirement lands, and the red
  #      now cites its card.
  # #3991 — invariant sharpened to what the finding actually was: an UNTYPED
  # subject carrying hasDomain (the product-* untyped-with-edges class, root:
  # product-instances.ttl absent from MODEL_SET, fixed in the same change).
  # Typed non-Product carriers (Skill/Gate/SubDomain, ~55 today) are a separate
  # vocabulary-alignment question — hasDomain's declared rdfs:domain is
  # Product|Document — owned by the #1772 naming/vocab convergence, not this
  # suite; asserting on them here would freeze a model call into a migration test.
  [ "$(count '?s chorus:hasDomain ?d . FILTER NOT EXISTS { ?s a ?anyType }')" -eq 0 ]
}
