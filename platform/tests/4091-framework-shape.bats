#!/usr/bin/env bats
# @test-type: contract — runs SHACL over fixtures written to BATS_TEST_TMPDIR against
# @domain: toolchain — the domain that holds Tool, ToolType and Framework (#4091)
# the repo's Tool, ToolType and Framework shapes (roles/kade/ontology/toolchain-4091.ttl); no store, no network.
# #4091 AC5 (Jeff 2026-10-20 17:06; Wren 17:31) — a Framework is a Tool that has
# operations, a Sensor, an ICD provider (its pluggable model) and wraps an
# upstream tool. A Tool without them is just a Tool: the shape tells them apart.
#
# Each NEGATIVE proof (#3734) puts a row in the state the shape exists to refuse
# and shows SHACL non-conformance; the controls show a well-formed row conforms.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  TTL="$REPO/roles/kade/ontology/toolchain-4091.ttl"
  SHAPES="$BATS_TEST_TMPDIR/shapes.nt"
  # only this card's shapes; sh:node CoreShape is dropped (its fields are optional
  # and it is defined elsewhere), as 4662 does
  riot --output=nt "$TTL" 2>/dev/null \
    | grep -E '^<https://jeffbridwell\.com/chorus#(Tool|ToolType|Framework)Shape' \
    | grep -vE 'shacl#(sparql|node)>' > "$SHAPES"
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:Framework rdfs:subClassOf chorus:Tool .
chorus:grafana-sensor a chorus:Sensor .
chorus:grafana-alert-ops a chorus:Skill .
chorus:principal-kade a chorus:Principal .
chorus:observe a chorus:ToolType ; rdfs:label "Observe" .
'
  FW='rdfs:label "Grafana" ; chorus:ownedBy chorus:principal-kade ; chorus:toolType chorus:observe'
}

# CONFORMS, VIOLATES (a real report saying sh:conforms false), or ERROR: an
# unparseable shapes graph must never read as a refusal.
verdict() {
  local out
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>&1)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"
  elif echo "$out" | grep -q "sh:conforms *false"; then echo "VIOLATES"
  else echo "ERROR"; fi
}

fixture() { printf '%s%s\n' "$PREFIX" "$1" > "$BATS_TEST_TMPDIR/$2.ttl"; echo "$BATS_TEST_TMPDIR/$2.ttl"; }

whole() { echo "chorus:grafana a chorus:Framework ; $FW ; chorus:hasOperation chorus:grafana-alert-ops ; chorus:hasSensor chorus:grafana-sensor ; chorus:icdProvider \"https://jeffbridwell.com/icd/provider/grafana/alert-rules\" ; chorus:wraps \"Grafana OSS\" ."; }

@test "the three shapes are in the file, each with a required field" {
  for s in ToolShape ToolTypeShape FrameworkShape; do
    grep -q "#$s>" "$SHAPES" || { echo "no $s"; return 1; }
  done
  grep -q 'minCount' "$SHAPES" || return 1
}

@test "control: a plain Tool with a name, an owner and a type conforms" {
  f=$(fixture "chorus:jq a chorus:Tool ; $FW ." t1)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "control: a Framework with ops, a Sensor, an ICD provider and wraps conforms" {
  f=$(fixture "$(whole)" f1)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

# NEGATIVE PROOF (AC5): the card's named refusal.
@test "a Framework with no Sensor is refused" {
  f=$(fixture "$(whole | sed 's/ chorus:hasSensor chorus:grafana-sensor ;//')" f2)
  grep -q hasSensor "$f" && return 1
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a Framework with no operation is refused" {
  f=$(fixture "$(whole | sed 's/ chorus:hasOperation chorus:grafana-alert-ops ;//')" f3)
  grep -q hasOperation "$f" && return 1
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a Framework with no ICD provider is refused" {
  f=$(fixture "$(whole | sed 's| chorus:icdProvider "[^"]*" ;||')" f4)
  grep -q icdProvider "$f" && return 1
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a Framework that wraps nothing is refused" {
  f=$(fixture "$(whole | sed 's/ ; chorus:wraps "Grafana OSS"//')" f5)
  grep -q wraps "$f" && return 1
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a Framework whose Sensor link points at something that is not a Sensor is refused" {
  f=$(fixture "$(whole | sed 's/chorus:hasSensor chorus:grafana-sensor/chorus:hasSensor chorus:grafana-alert-ops/')" f6)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

# the prod Tool rows (2026-09-26) carry no owner; a plain Tool must still conform
@test "control: a plain Tool with no owner conforms, as the prod rows do" {
  f=$(fixture 'chorus:listen a chorus:Tool ; rdfs:label "/listen" , "listen" ; chorus:toolType chorus:observe .' t2)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "a Framework with no owner is refused" {
  f=$(fixture "$(whole | sed 's/ chorus:ownedBy chorus:principal-kade ;//')" f8)
  grep -q ownedBy "$f" && return 1
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a ToolType with no name is refused; with one it conforms" {
  f=$(fixture 'chorus:x a chorus:ToolType .' tt1)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
  f=$(fixture 'chorus:x a chorus:ToolType ; rdfs:label "Observe" .' tt2)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "Framework is a Tool and joins the toolchain vocabulary" {
  nt=$(riot --output=nt "$TTL" 2>/dev/null)
  C='https://jeffbridwell.com/chorus#'
  grep -qF "<${C}Framework> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <${C}Tool> ." <<<"$nt" || return 1
  grep -qF "<${C}toolchain> <${C}definesVocabulary> <${C}Framework> ." <<<"$nt" || return 1
}

# NEGATIVE PROOF: an ICD provider that is not an ICD provider IRI is refused
# (the first draft of this shape expected urn:gathering:icd/provider/, which no
# provider uses: they live under the ICD files' @base).
@test "a Framework whose ICD provider is not a provider IRI is refused" {
  f=$(fixture "$(whole | sed 's|https://jeffbridwell.com/icd/provider/grafana/alert-rules|grafana|')" f7)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "the Grafana ICD provider the Framework names exists in the ICD instances" {
  nt=$(riot --output=nt "$REPO/building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl" 2>/dev/null)
  grep -qF '<https://jeffbridwell.com/icd/provider/grafana/alert-rules> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:gathering:icd#Provider> .' <<<"$nt" || return 1
  [ "$(grep -cF '<urn:gathering:icd#fromProvider> <https://jeffbridwell.com/icd/provider/grafana/alert-rules>' <<<"$nt")" -eq 11 ] || return 1
}

# Silas 10-21: the 7 Tool + 5 ToolType rows (seeded from designing/data) must
# pass the new shapes. My first ToolShape required ownedBy and failed all 7.
# Then option (b): the 10 bare IRIs (chorus:listen, chorus:Sense...) listed but
# 404'd on read, and chorus:Memory was both a ToolType row and a class. The
# seeds now mint every row kind-prefixed, as the door reads them.
SEEDS() { echo "$REPO/designing/data/tool-instances.ttl $REPO/designing/data/tool-type-instances.ttl"; }

@test "the seeded Tool and ToolType rows conform" {
  riot --output=ttl $(SEEDS) > "$BATS_TEST_TMPDIR/seeds.ttl" 2>/dev/null || return 1
  [ "$(riot --output=nt "$BATS_TEST_TMPDIR/seeds.ttl" 2>/dev/null | grep -cE 'rdf-syntax-ns#type> <https://jeffbridwell.com/chorus#Tool(Type)?> ')" -eq 12 ] || return 1
  [ "$(verdict "$BATS_TEST_TMPDIR/seeds.ttl")" = CONFORMS ] || return 1
}

# NEGATIVE PROOF: a bare Tool or ToolType IRI (the door cannot read it) is caught.
bare_rows() { riot --output=nt "$@" 2>/dev/null | grep -E 'rdf-syntax-ns#type> <https://jeffbridwell.com/chorus#Tool(Type)?> ' | cut -d' ' -f1 | grep -vE '#tool-' || true; }

@test "every seeded Tool and ToolType IRI is kind-prefixed (tool-, tool-type-)" {
  [ -z "$(bare_rows $(SEEDS))" ] || { bare_rows $(SEEDS); return 1; }
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:listen a chorus:Tool .\n' > "$BATS_TEST_TMPDIR/bare.ttl"
  [ -n "$(bare_rows "$BATS_TEST_TMPDIR/bare.ttl")" ] || return 1
}

@test "no ToolType shares an IRI with a class (chorus:Memory was both)" {
  nt=$(riot --output=nt $(SEEDS) "$REPO"/roles/*/ontology/*.ttl 2>/dev/null)
  for t in $(grep -F 'rdf-syntax-ns#type> <https://jeffbridwell.com/chorus#ToolType> ' <<<"$nt" | cut -d' ' -f1); do
    ! grep -qF "$t <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class>" <<<"$nt" || { echo "$t is a class"; return 1; }
  done
}

@test "every property shape in the file is named and has a path; no blank nodes" {
  nt=$(riot --output=nt "$TTL" 2>/dev/null)
  ! grep -q '^_:' <<<"$nt" || return 1
  ps=$(grep -F '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/shacl#PropertyShape>' <<<"$nt" | cut -d' ' -f1 | sort -u)
  [ "$(wc -l <<<"$ps")" -ge 15 ] || return 1
  for s in $ps; do grep -qF "$s <http://www.w3.org/ns/shacl#path>" <<<"$nt" || { echo "no path: $s"; return 1; }; done
}
