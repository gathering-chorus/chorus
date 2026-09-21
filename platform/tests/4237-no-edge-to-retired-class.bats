#!/usr/bin/env bats
# @test-type: unit — static guards over the model file, the kinds table, the shapes
# file and the .sparql files. No service, no store: it reads what is checked in, so
# it answers the same in a werk, on canonical and in the nightly.
# #4237 — an edge may not be constrained to a class the DAL cannot mint.
#
# The failure this catches, in Jeff's terms: you try to create a thing and the
# door refuses, because a shape requires an edge pointing at a class that was
# retired. #4010 hit it on Document (hasDomain -> SubDomain) and fixed that one
# shape. AuthBoundary still has it on betweenDomainA/betweenDomainB, so no
# AuthBoundary can be created at all.
#
# The check is: no sh:class in the model names a class that has no DAL kind.
# Asserted with simple commands, never [[ ]] — on bash 3.2 a failing [[ that is
# not the last line of a test passes silently (91 of 223 suites carried that).

setup() {
  # Resolve from the test file, NEVER $CHORUS_ROOT. CHORUS_ROOT points at
  # canonical, so a werk-run check would measure the tree it is not changing and
  # report on code nobody edited — the #3701 ratchet-measured-CANONICAL defect.
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
  SHAPES="$ROOT/platform/api/src/sparql/shapes.ttl"
  KINDS="$ROOT/platform/services/athena-model/src/lib.rs"
}

# Every class named by an sh:class constraint, one per line.
sh_class_targets() {
  grep -oE 'sh:class[[:space:]]+chorus:[A-Za-z]+' "$MODEL" \
    | awk '{print $2}' | sed 's/^chorus://' | sort -u
}

# Every class the DAL can mint, from the KINDS table.
dal_classes() {
  sed -n '/const KINDS/,/\];/p' "$KINDS" \
    | grep -oE '"[A-Za-z]+"[[:space:]]*,[[:space:]]*(true|false)' \
    | grep -oE '^"[A-Za-z]+"' | tr -d '"' | sort -u
}

@test "the model file and the kinds table are both readable" {
  test -f "$MODEL"
  test -f "$KINDS"
  n=$(sh_class_targets | wc -l | tr -d ' ')
  test "$n" -gt 0
}

@test "no sh:class names a class the DAL cannot mint" {
  orphans=$(comm -23 <(sh_class_targets) <(dal_classes))
  if test -n "$orphans"; then
    echo "sh:class targets with no DAL kind:"
    echo "$orphans"
    echo "-- an edge required by a shape but unmintable means the create fails closed"
    return 1
  fi
}

@test "NEGATIVE PROOF: the check fails when a shape names a retired class" {
  fixture="$BATS_TEST_TMPDIR/fixture.ttl"
  cp "$MODEL" "$fixture"
  printf '\nchorus:FixtureShape a sh:NodeShape ;\n    sh:property [ sh:path chorus:fixtureEdge ; sh:class chorus:NoSuchRetiredClass ; ] .\n' >> "$fixture"
  MODEL="$fixture"
  orphans=$(comm -23 <(sh_class_targets) <(dal_classes))
  echo "$orphans" | grep -q "NoSuchRetiredClass"
}

@test "NEGATIVE PROOF: a class the DAL CAN mint is not reported" {
  # grep -qx, not grep -q: a substring match would let "SubDomain" answer for
  # "Domain" and the test would pass for the wrong reason.
  dal_classes | grep -qx "Domain"
  fixture="$BATS_TEST_TMPDIR/ok.ttl"
  cp "$MODEL" "$fixture"
  printf '\nchorus:OkShape a sh:NodeShape ;\n    sh:property [ sh:path chorus:okEdge ; sh:class chorus:Domain ; ] .\n' >> "$fixture"
  MODEL="$fixture"
  orphans=$(comm -23 <(sh_class_targets) <(dal_classes))
  run bash -c "echo \"$orphans\" | grep -qx Domain"
  test "$status" -ne 0
}

# --- sh:targetClass, the second half of the same defect --------------------
#
# An sh:class that names a retired class refuses a write. An sh:targetClass that
# names one is quieter and worse: it matches nothing, never fires, and counts as
# a pass on every run. #4237 deleted four of these (SubProductParentShape,
# SubProductDomainShape, SubDomainParentShape, SubDomainInstancesShape) — two for
# a class retired by #3603 and two for a class retired by #3509. All four had been
# reporting clean for months without being able to report anything else.

# Every class a shape targets, one per line.
target_classes() {
  grep -ohE 'sh:targetClass[[:space:]]+chorus:[A-Za-z]+' "$MODEL" "$SHAPES" \
    | awk '{print $2}' | sed 's/^chorus://' | sort -u
}

# Every class DECLARED in the model.
declared_classes() {
  grep -oE '^chorus:[A-Za-z]+[[:space:]]+a[[:space:]]+owl:Class' "$MODEL" \
    | awk '{print $1}' | sed 's/^chorus://' | sort -u
}

@test "the shapes file is readable and declares classes to check against" {
  test -f "$SHAPES"
  n=$(declared_classes | wc -l | tr -d ' ')
  test "$n" -gt 10
}

@test "no sh:targetClass names a class the model does not declare" {
  orphans=$(comm -23 <(target_classes) <(declared_classes))
  if test -n "$orphans"; then
    echo "sh:targetClass naming an undeclared class:"
    echo "$orphans"
    echo "-- a shape on a class that does not exist matches nothing and passes forever"
    return 1
  fi
}

@test "NEGATIVE PROOF: the targetClass check fails on a shape for a retired class" {
  fixture="$BATS_TEST_TMPDIR/shapes.ttl"
  cp "$SHAPES" "$fixture"
  printf '\nchorus:GhostShape a sh:NodeShape ;\n  sh:targetClass chorus:SubProduct ;\n  sh:property [ sh:path chorus:anything ; sh:minCount 1 ] .\n' >> "$fixture"
  SHAPES="$fixture"
  orphans=$(comm -23 <(target_classes) <(declared_classes))
  echo "$orphans" | grep -qx "SubProduct"
}

@test "NEGATIVE PROOF: a shape on a declared class is not reported" {
  declared_classes | grep -qx "Product"
  fixture="$BATS_TEST_TMPDIR/ok-shapes.ttl"
  cp "$SHAPES" "$fixture"
  printf '\nchorus:RealShape a sh:NodeShape ;\n  sh:targetClass chorus:Product ;\n  sh:property [ sh:path chorus:anything ; sh:minCount 1 ] .\n' >> "$fixture"
  SHAPES="$fixture"
  orphans=$(comm -23 <(target_classes) <(declared_classes))
  run bash -c "echo \"$orphans\" | grep -qx Product"
  test "$status" -ne 0
}

# --- row queries must not read the schema graph ---------------------------
#
# Jeff, 2026-09-03: no rows in the ontology graph; every row lives in its own
# domain graph. A query that asks urn:chorus:ontology for instances therefore asks
# a graph that must be empty of them — and answers 0 without failing. That is what
# emptied /api/athena/subdomains and /api/athena/products this week: the rows
# moved, the queries did not, and the endpoints returned [] as a normal answer.
#
# This checks the .sparql files that serve rows. It does not try to police every
# mention of the graph name — schema queries legitimately read it.

row_queries_reading_the_schema_graph() {
  local d="$ROOT/platform/api/src/sparql"
  for f in "$d"/*.sparql; do
    test -f "$f" || continue
    # Strip comments first. A line recording that this file USED to read the schema
    # graph is history, not a query — matching it would make the gate red forever
    # on its own fix note, and the obvious "fix" would be to delete the note.
    sed 's/#.*//' "$f" | grep -q "urn:chorus:ontology" || continue
    # a row query binds an instance: "?x a chorus:SomeClass"
    grep -qE '\?[a-z]+ a chorus:[A-Z]' "$f" || continue
    basename "$f"
  done
}

@test "no row-serving SPARQL file reads urn:chorus:ontology" {
  offenders=$(row_queries_reading_the_schema_graph)
  if test -n "$offenders"; then
    echo "row queries pointed at the SCHEMA graph:"
    echo "$offenders"
    echo "-- these return [] as a normal answer when the rows are elsewhere"
    return 1
  fi
}

@test "NEGATIVE PROOF: the row-query check fails on a planted schema-graph query" {
  d="$BATS_TEST_TMPDIR/sparql"; mkdir -p "$d"
  cp "$ROOT/platform/api/src/sparql/"*.sparql "$d/" 2>/dev/null || true
  printf 'SELECT ?x WHERE { GRAPH <urn:chorus:ontology> { ?x a chorus:Machine } }\n' > "$d/planted.sparql"
  ROOT_SAVE="$ROOT"; ROOT="$BATS_TEST_TMPDIR/fakeroot"
  mkdir -p "$ROOT/platform/api/src"; ln -sfn "$d" "$ROOT/platform/api/src/sparql"
  offenders=$(row_queries_reading_the_schema_graph)
  ROOT="$ROOT_SAVE"
  echo "$offenders" | grep -q "planted.sparql"
}
