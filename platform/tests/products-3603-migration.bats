#!/usr/bin/env bats
# @test-type: integration
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

@test "the chorus child products are typed chorus:Product and partOf chorusProduct" {
  for p in product-loom product-athena product-werk product-clearing product-convergence; do
    [ "$(ask "chorus:$p a chorus:Product ; chorus:partOf chorus:chorusProduct")" = "True" ]
  done
  [ "$(ask 'chorus:borgProduct a chorus:Product ; chorus:partOf chorus:chorusProduct')" = "True" ]
}

@test "spine and pulse are typed chorus:Product children of product-clearing" {
  for p in product-spine product-pulse; do
    [ "$(ask "chorus:$p a chorus:Product ; chorus:partOf chorus:product-clearing")" = "True" ]
  done
}

@test "quality-product and the product-borg dup are retired (gone)" {
  [ "$(ask 'chorus:quality-product ?p ?o')" = "False" ]
  [ "$(ask 'chorus:product-borg ?p ?o')" = "False" ]
}

@test "no subject points hasDomain at anything while itself untyped as Product" {
  [ "$(count '?s chorus:hasDomain ?d . FILTER NOT EXISTS { ?s a chorus:Product }')" -eq 0 ]
}
