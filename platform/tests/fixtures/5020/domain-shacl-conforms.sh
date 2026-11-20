#!/bin/sh
# @domain: policies
# #5020 AC5 — writes into $1 one violation of domain-shacl-conforms for domain x: data.trig, where
# a row of x lacks the label its shape requires (y's row has it), and shapes.ttl, the shape.
# Run the counter with SHACL_DATA=$1/data.trig SHACL_SHAPES=$1/shapes.ttl.
set -e
cat > "$1/shapes.ttl" <<'TTL'
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
c:ThingShape a sh:NodeShape ; sh:targetClass c:Thing ;
    sh:property [ sh:path rdfs:label ; sh:minCount 1 ] .
TTL
cat > "$1/data.trig" <<'TTL'
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:chorus:domains:domains> { c:x a c:Domain . c:y a c:Domain . }
<urn:chorus:domains:x> { c:x-one a c:Thing . }
<urn:chorus:domains:y> { c:y-one a c:Thing ; rdfs:label "y one" . }
TTL
