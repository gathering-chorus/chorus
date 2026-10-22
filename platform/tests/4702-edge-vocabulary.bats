#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: ontology
# @card: #4702
# @owner: wren
#
# #4702 AC4 — the edges between products and domains, settled 2026-10-22 (Wren +
# Silas 15:59, Jeff 16:03): hasDomain is a domain's one home; composes builds on a
# domain homed elsewhere; consumes reads a domain or service. Declarations and
# shapes live in roles/silas/ontology/chorus.ttl. Each rule ships with a row built
# to break it.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPES="$ROOT/roles/silas/ontology/chorus.ttl"

setup() {
  command -v shacl >/dev/null 2>&1 || { echo "shacl (Jena) not installed: cannot prove the edge rules" >&2; return 1; }
  T="$BATS_TEST_TMPDIR"
  cat > "$T/rows.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:d-cards a chorus:Domain .
chorus:d-awareness a chorus:Domain .
chorus:s-pulse a chorus:Service .
chorus:m-library a chorus:Machine .
chorus:vs-chorus a chorus:ValueStream .
chorus:p-good a chorus:Product ; chorus:composes chorus:d-cards ; chorus:consumes chorus:s-pulse .
chorus:p-composes-service a chorus:Product ; chorus:composes chorus:s-pulse .
chorus:d-good a chorus:Domain ; chorus:consumes chorus:d-cards ; chorus:builtBy chorus:vs-chorus ; chorus:runsOn chorus:m-library .
chorus:d-consumes-machine a chorus:Domain ; chorus:consumes chorus:m-library .
chorus:d-builtby-product a chorus:Domain ; chorus:builtBy chorus:p-good .
EOF
  shacl validate --shapes "$SHAPES" --data "$T/rows.ttl" > "$T/report.txt" 2>&1 || true
  awk '/sh:focusNode/ {f=$2} /sh:resultPath/ {print f, $2}' "$T/report.txt" | tr -d ';' > "$T/results.txt"
}

refused() { grep -qx "chorus:$1 chorus:$2" "$T/results.txt"; }

@test "composes is declared Product to Domain, described, and may cross hierarchies" {
  grep -A4 '^chorus:composes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:domain chorus:Product' || return 1
  grep -A4 '^chorus:composes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:range chorus:Domain' || return 1
  grep -A4 '^chorus:composes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:comment' || return 1
  grep -q '^chorus:composes      chorus:crossesHierarchy' "$SHAPES" || return 1
}

@test "consumes is declared from Product or Domain to Domain or Service (no stale Service-only range)" {
  grep -A4 '^chorus:consumes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:domain \[ owl:unionOf ( chorus:Product chorus:Domain ) \]' || return 1
  grep -A4 '^chorus:consumes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:range  \[ owl:unionOf ( chorus:Domain chorus:Service ) \]' || return 1
  ! grep -A4 '^chorus:consumes a owl:ObjectProperty' "$SHAPES" | grep -q 'rdfs:range chorus:Service' || return 1
}

@test "a product that composes a domain and consumes a service passes, as does a domain's consumes, builtBy and runsOn" {
  for e in "p-good composes" "p-good consumes" "d-good consumes" "d-good builtBy" "d-good runsOn"; do
    ! refused $e || { echo "refused: $e"; cat "$T/results.txt"; return 1; }
  done
}

@test "negative proof: a product that composes a service is refused" {
  refused p-composes-service composes || { cat "$T/results.txt"; return 1; }
}

@test "negative proof: a domain that consumes a machine is refused" {
  refused d-consumes-machine consumes || { cat "$T/results.txt"; return 1; }
}

@test "negative proof: a domain built by a product, not a value stream, is refused" {
  refused d-builtby-product builtBy || { cat "$T/results.txt"; return 1; }
}

# AC5 — the closed shapes hold for the model's own rows, and refuse a planted field.
closed_results() {
  shacl validate --shapes "$SHAPES" --data "$1" 2>&1 |
    awk '/sh:focusNode/ {f=$2} /sourceConstraintComponent/ {print f, $2}' | tr -d ';' | grep ClosedConstraint || true
}

@test "every Product and Domain row in the model source passes the closed shapes (CoreShape paths and stamps ignored)" {
  run closed_results "$SHAPES"
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "negative proof: a model row with one unknown field is refused by the closed shape" {
  { cat "$SHAPES"; printf '\nchorus:x-domain a chorus:Domain ; chorus:notAField "x" .\nchorus:x-product a chorus:Product ; chorus:notAField "x" .\n'; } > "$T/planted.ttl"
  run closed_results "$T/planted.ttl"
  [[ "$output" == *"chorus:x-domain sh:ClosedConstraintComponent"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"chorus:x-product sh:ClosedConstraintComponent"* ]] || return 1
}

