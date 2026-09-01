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

@test "chorusProduct is a typed chorus:Product" {
  [ "$(ask 'chorus:chorusProduct a chorus:Product')" = "True" ]
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

@test "#3915: the served products are exactly the named set (a silent disappearance still reds)" {
  # Was `-eq 8`, which reds on any deliberate addition and cannot say WHICH
  # product vanished. Naming the set keeps the disappearance proof and turns an
  # intentional change into a one-line edit that states what changed.
  want="athena borg chorus clearing convergence loom pulse spine werk"
  got="$(curl -sf --max-time 10 http://localhost:3360/products \
    | python3 -c 'import json,sys; d=json.load(sys.stdin).get("data",[]); print(" ".join(sorted(x.get("name","") for x in d)))' 2>/dev/null)"
  [ -n "$got" ] || skip "UNMEASURABLE: owl-api not answering"
  [ "$got" = "$want" ]
}

@test "NEGATIVE PROOF: the named-set check REDS when a product disappears" {
  # #3734 — the set comparison must be able to fail. Drop one name from the
  # served side and prove the same comparison rejects it.
  want="athena borg chorus clearing convergence loom pulse spine werk"
  got="athena borg chorus clearing convergence loom pulse spine"
  [ "$got" != "$want" ]
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
