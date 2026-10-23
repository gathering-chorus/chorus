#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: ontology
# @card: #4702
# @owner: wren
#
# #4702 — Jeff 2026-10-22 16:02: "lets make it a flag these are high dependency
# domains and services not a product per se". The flag lives in
# roles/silas/ontology/chorus.ttl on DomainShape and ServiceShape: one boolean,
# written true or false, at most once. Absent is undecided (the radius tools
# flag it), so the shape refuses only a wrong value, never a missing one yet.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPES="$ROOT/roles/silas/ontology/chorus.ttl"

setup() {
  # A missing validator must fail, never skip: a skipped negative proof reads green.
  command -v shacl >/dev/null 2>&1 || { echo "shacl (Jena) not installed: cannot prove the platform rule" >&2; return 1; }
  T="$BATS_TEST_TMPDIR"
  cat > "$T/rows.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:dom-true    a chorus:Domain ; chorus:platform true .
chorus:dom-false   a chorus:Domain ; chorus:platform false .
chorus:dom-absent  a chorus:Domain .
chorus:dom-word    a chorus:Domain ; chorus:platform "yes" .
chorus:dom-two     a chorus:Domain ; chorus:platform true , false .
chorus:svc-true    a chorus:Service ; chorus:platform true .
chorus:svc-word    a chorus:Service ; chorus:platform "yes" .
EOF
  shacl validate --shapes "$SHAPES" --data "$T/rows.ttl" > "$T/report.txt" 2>&1 || true
  # one "<focus> <path>" line per result, so each row's platform result is greppable
  awk '/sh:focusNode/ {f=$2} /sh:resultPath/ {print f, $2}' "$T/report.txt" | tr -d ';' > "$T/results.txt"
}

refused() { grep -qx "chorus:$1 chorus:platform" "$T/results.txt"; }

@test "the platform flag is declared, boolean and described" {
  grep -q '^chorus:platform a owl:DatatypeProperty , owl:FunctionalProperty' "$SHAPES" || return 1
  grep -A2 '^chorus:platform a owl:DatatypeProperty' "$SHAPES" | grep -q 'rdfs:range xsd:boolean' || return 1
  grep -A2 '^chorus:platform a owl:DatatypeProperty' "$SHAPES" | grep -q 'rdfs:comment' || return 1
}

@test "true, false and absent pass the platform rule on Domain and Service" {
  for row in dom-true dom-false dom-absent svc-true; do
    ! refused "$row" || { echo "$row refused on platform"; cat "$T/results.txt"; return 1; }
  done
}

@test "negative proof: a word instead of a boolean is refused on Domain and Service" {
  refused dom-word || { cat "$T/results.txt"; return 1; }
  refused svc-word || return 1
}

@test "negative proof: two platform values on one domain are refused" {
  refused dom-two || { cat "$T/results.txt"; return 1; }
}

@test "the Service read serves the flag (exposure public), or the door withholds it" {
  # the ServiceShape block only: from its declaration to the next shape
  awk '/^chorus:ServiceShape a sh:NodeShape/ {on=1} on && /^chorus:[A-Za-z]+Shape a sh:NodeShape/ && !/ServiceShape/ {on=0} on' "$SHAPES" > "$T/service-shape.ttl"
  grep -q 'sh:path chorus:platform' "$T/service-shape.ttl" || { echo "ServiceShape has no platform property"; return 1; }
  grep 'sh:path chorus:platform' "$T/service-shape.ttl" | grep -q 'chorus:exposure chorus:public' || return 1
}
