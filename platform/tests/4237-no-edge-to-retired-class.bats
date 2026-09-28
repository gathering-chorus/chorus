#!/usr/bin/env bats
# @test-type: fitness — static guards over the model file, the DAL binary, the shapes
# @domain: knowledge — the product domain this suite guards (#4334)
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
# #4336: every set below comes from RUNNING something, not from grepping source:
#   - the model's sh:class / sh:targetClass / owl:Class sets: SPARQL (arq) over the TTL
#   - the DAL's mintable classes: the athena-model binary built from THIS tree, asked
#     for its kinds (the unknown-kind refusal lists them) and then asked to dry-run a
#     row of each kind, which prints the class it would mint. Store unreachable, so
#     only the hand table answers — the strict set, as before.
#   - row queries: every .sparql executed against a fixture store whose ONLY rows sit
#     in urn:chorus:ontology; a query that returns one of them reads rows from the
#     schema graph.
# Asserted with simple commands, never [[ ]] — on bash 3.2 a failing [[ that is
# not the last line of a test passes silently (91 of 223 suites carried that).

NS="https://jeffbridwell.com/chorus#"

setup_file() {
  # Resolve from the test file, NEVER $CHORUS_ROOT (the #3701 ratchet-measured-CANONICAL defect).
  local root; root="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  local crate="$root/platform/services/athena-model"
  # The binary of THIS tree: the tree's release build when it is newer than every
  # source file, otherwise a fresh build into this run's temp dir (never into the tree).
  local bin="$crate/target/release/athena-model"
  if [ ! -x "$bin" ] || [ -n "$(find "$crate/src" "$crate/Cargo.toml" "$root/platform/services/chorus-oidc/src" -newer "$bin" -type f 2>/dev/null | head -1)" ]; then
    bin=""
    if command -v cargo >/dev/null 2>&1; then
      cargo build --release --quiet --manifest-path "$crate/Cargo.toml" --target-dir "$BATS_FILE_TMPDIR/target" >/dev/null 2>&1 \
        && bin="$BATS_FILE_TMPDIR/target/release/athena-model"
    fi
  fi
  export AM_BIN="$bin"
}

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
  SHAPES="$ROOT/platform/api/src/sparql/shapes.ttl"
  SPARQL_DIR="$ROOT/platform/api/src/sparql"
  command -v arq >/dev/null 2>&1 || skip "UNMEASURED — arq (Apache Jena) not installed; the model sets come from SPARQL over the TTL (#4336)"
}

need_dal() {
  [ -n "$AM_BIN" ] && [ -x "$AM_BIN" ] || { echo "athena-model could not be built from this tree (cargo build failed or cargo absent)"; return 1; }
}

# local names of the chorus-namespace IRIs a one-variable SELECT returns
arq_locals() {
  local q="$1"; shift
  local args=() f
  for f in "$@"; do args+=(--data "$f"); done
  printf '%s\n' "$q" > "$BATS_TEST_TMPDIR/q.rq"
  arq "${args[@]}" --query "$BATS_TEST_TMPDIR/q.rq" --results=TSV > "$BATS_TEST_TMPDIR/q.out" || { echo "arq failed on: $*" >&2; return 1; }
  tail -n +2 "$BATS_TEST_TMPDIR/q.out" | grep "^<$NS" | sed 's/.*#//; s/>$//' | sort -u
}

# Every class named by an sh:class constraint, one per line.
sh_class_targets() {
  arq_locals "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT DISTINCT ?c WHERE { ?s sh:class ?c FILTER(STRSTARTS(STR(?c), \"$NS\")) }" "$MODEL"
}

# Every class the DAL can mint: ask the binary for its kinds, then for the class of each.
dal_classes() {
  need_dal || return 1
  local env=(env -i PATH=/usr/bin:/bin HOME="$BATS_TEST_TMPDIR" CHORUS_CONTEXT=test CHORUS_FUSEKI=http://127.0.0.1:9)
  local refusal kinds k
  refusal=$("${env[@]}" "$AM_BIN" add --kind no-such-kind-4336 --name probe --dry-run 2>&1 || true)
  kinds=$(printf '%s\n' "$refusal" | sed -n 's/.*unknown-kind.* Known: //p' | tr ',' '\n' | tr -d ' ')
  [ -n "$kinds" ] || { echo "athena-model listed no kinds: $refusal" >&2; return 1; }
  for k in $kinds; do
    "${env[@]}" "$AM_BIN" add --kind "$k" --name probe --dry-run 2>/dev/null \
      | grep -oE "> a <${NS}[A-Za-z]+>" | sed 's/.*#//; s/>$//'
  done | sort -u
}

@test "the model file is readable and the DAL binary answers with its kinds" {
  test -f "$MODEL"
  n=$(sh_class_targets | wc -l | tr -d ' ')
  test "$n" -gt 0
  d=$(dal_classes | wc -l | tr -d ' ')
  test "$d" -gt 10
}

@test "no sh:class names a class the DAL cannot mint" {
  sh_class_targets > "$BATS_TEST_TMPDIR/targets" || return 1
  dal_classes > "$BATS_TEST_TMPDIR/dal" || return 1
  orphans=$(comm -23 "$BATS_TEST_TMPDIR/targets" "$BATS_TEST_TMPDIR/dal")
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
  sh_class_targets > "$BATS_TEST_TMPDIR/targets" || return 1
  dal_classes > "$BATS_TEST_TMPDIR/dal" || return 1
  orphans=$(comm -23 "$BATS_TEST_TMPDIR/targets" "$BATS_TEST_TMPDIR/dal")
  echo "$orphans" | grep -qx "NoSuchRetiredClass"
}

@test "NEGATIVE PROOF: a class the DAL CAN mint is not reported" {
  # grep -qx, not grep -q: a substring match would let "SubDomain" answer for
  # "Domain" and the test would pass for the wrong reason.
  dal_classes > "$BATS_TEST_TMPDIR/dal" || return 1
  grep -qx "Domain" "$BATS_TEST_TMPDIR/dal"
  # a class whose kind is not its kebab form still comes back under its own name
  grep -qx "APISurface" "$BATS_TEST_TMPDIR/dal"
  fixture="$BATS_TEST_TMPDIR/ok.ttl"
  cp "$MODEL" "$fixture"
  printf '\nchorus:OkShape a sh:NodeShape ;\n    sh:property [ sh:path chorus:okEdge ; sh:class chorus:Domain ; ] .\n' >> "$fixture"
  MODEL="$fixture"
  sh_class_targets > "$BATS_TEST_TMPDIR/targets" || return 1
  grep -qx "Domain" "$BATS_TEST_TMPDIR/targets"
  run grep -qx Domain <(comm -23 "$BATS_TEST_TMPDIR/targets" "$BATS_TEST_TMPDIR/dal")
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
  arq_locals "PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT DISTINCT ?c WHERE { ?s sh:targetClass ?c FILTER(STRSTARTS(STR(?c), \"$NS\")) }" "$MODEL" "$SHAPES"
}

# Every class DECLARED in the model.
declared_classes() {
  arq_locals "PREFIX owl: <http://www.w3.org/2002/07/owl#> SELECT DISTINCT ?c WHERE { ?c a owl:Class FILTER(STRSTARTS(STR(?c), \"$NS\")) }" "$MODEL"
}

@test "the shapes file is readable and declares classes to check against" {
  test -f "$SHAPES"
  n=$(declared_classes | wc -l | tr -d ' ')
  test "$n" -gt 10
}

@test "no sh:targetClass names a class the model does not declare" {
  target_classes > "$BATS_TEST_TMPDIR/tc" || return 1
  declared_classes > "$BATS_TEST_TMPDIR/decl" || return 1
  orphans=$(comm -23 "$BATS_TEST_TMPDIR/tc" "$BATS_TEST_TMPDIR/decl")
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
  target_classes > "$BATS_TEST_TMPDIR/tc" || return 1
  declared_classes > "$BATS_TEST_TMPDIR/decl" || return 1
  comm -23 "$BATS_TEST_TMPDIR/tc" "$BATS_TEST_TMPDIR/decl" | grep -qx "SubProduct"
}

@test "NEGATIVE PROOF: a shape on a declared class is not reported" {
  declared_classes > "$BATS_TEST_TMPDIR/decl" || return 1
  grep -qx "Product" "$BATS_TEST_TMPDIR/decl"
  fixture="$BATS_TEST_TMPDIR/ok-shapes.ttl"
  cp "$SHAPES" "$fixture"
  printf '\nchorus:RealShape a sh:NodeShape ;\n  sh:targetClass chorus:Product ;\n  sh:property [ sh:path chorus:anything ; sh:minCount 1 ] .\n' >> "$fixture"
  SHAPES="$fixture"
  target_classes > "$BATS_TEST_TMPDIR/tc" || return 1
  grep -qx "Product" "$BATS_TEST_TMPDIR/tc"
  run grep -qx Product <(comm -23 "$BATS_TEST_TMPDIR/tc" "$BATS_TEST_TMPDIR/decl")
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
# #4336: each .sparql file is EXECUTED against a fixture store holding one ghost row
# of every declared class, and nothing else, all inside urn:chorus:ontology. A query
# that returns a ghost reads rows from the schema graph. Schema queries (counts,
# ownedBy edges) return no ghost; comments are never executed, so a history note
# naming the graph cannot turn the gate red.

ghost_store() {
  local out="$BATS_TEST_TMPDIR/ghost.trig"
  declared_classes > "$BATS_TEST_TMPDIR/decl" || return 1
  {
    echo 'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>'
    echo 'GRAPH <urn:chorus:ontology> {'
    while IFS= read -r c; do
      printf '<urn:ghost:%s> a <%s%s> ; rdfs:label "ghost %s" .\n' "$c" "$NS" "$c" "$c"
    done < "$BATS_TEST_TMPDIR/decl"
    echo '}'
  } > "$out"
  echo "$out"
}

row_queries_reading_the_schema_graph() {
  local d="$1" store="$2" f r
  for f in "$d"/*.sparql; do
    test -f "$f" || continue
    # the API fills $PLACEHOLDERS before it runs a query; fill them with a ghost IRI
    sed 's/\$[A-Z_][A-Z_]*/urn:ghost:param/g' "$f" > "$BATS_TEST_TMPDIR/run.rq"
    if ! r=$(arq --data "$store" --query "$BATS_TEST_TMPDIR/run.rq" --results=TSV 2>&1); then
      echo "$(basename "$f") (does not parse — cannot be measured)"; continue
    fi
    printf '%s\n' "$r" | grep -q '<urn:ghost:[A-Za-z]*>' && basename "$f"
  done
  return 0
}

@test "no row-serving SPARQL file reads urn:chorus:ontology" {
  store=$(ghost_store) || return 1
  offenders=$(row_queries_reading_the_schema_graph "$SPARQL_DIR" "$store")
  if test -n "$offenders"; then
    echo "row queries pointed at the SCHEMA graph:"
    echo "$offenders"
    echo "-- these return [] as a normal answer when the rows are elsewhere"
    return 1
  fi
}

@test "NEGATIVE PROOF: the row-query check fails on a planted schema-graph query" {
  store=$(ghost_store) || return 1
  d="$BATS_TEST_TMPDIR/sparql"; mkdir -p "$d"
  cp "$SPARQL_DIR/"*.sparql "$d/" 2>/dev/null || true
  printf 'PREFIX chorus: <%s>\nSELECT ?x WHERE { GRAPH <urn:chorus:ontology> { ?x a chorus:Machine } }\n' "$NS" > "$d/planted.sparql"
  # and a schema query that merely NAMES the graph in a comment and counts triples: not a row query
  printf '# was: GRAPH <urn:chorus:ontology> { ?x a chorus:Machine }\nSELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:ontology> { ?s ?p ?o } }\n' > "$d/schema-note.sparql"
  offenders=$(row_queries_reading_the_schema_graph "$d" "$store")
  echo "$offenders" | grep -qx "planted.sparql"
  run grep -qx "schema-note.sparql" <(echo "$offenders")
  test "$status" -ne 0
}
