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

whole() { echo "chorus:grafana a chorus:Framework ; $FW ; chorus:hasOperation chorus:grafana-alert-ops ; chorus:hasSensor chorus:grafana-sensor ; chorus:icdProvider \"urn:gathering:icd/provider/grafana/alert-rules\" ; chorus:wraps \"Grafana OSS\" ."; }

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

@test "a Tool with no owner is refused" {
  f=$(fixture 'chorus:jq a chorus:Tool ; rdfs:label "jq" ; chorus:toolType chorus:observe .' t2)
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
