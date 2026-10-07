#!/usr/bin/env bats
# @test-type: unit — validates TTL files with Jena's shacl CLI; no store, no network.
# @domain: principles — the product domain this suite guards (#4334)
#
# #4186 — Jeff, 2026-09-16: "use principles, the class and shacl is shallow".
# #4358 — Jeff, 2026-10-07: "i want the principles to be only pc and mapped to xp
# practices". The shape is the 14 Hemenway principles; every field is required
# and described, the row name and the citation are formats. Two proofs: the real
# rows conform, and a fixture built to violate every rule is REFUSED, each
# violation named.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/principles-3749.ttl"
PC="$ROOT/roles/wren/ontology/principles-instances-3749.ttl"
FIXTURE="$ROOT/platform/tests/fixtures/principles-4186-violations.ttl"
FIXUP="$ROOT/platform/tests/fixtures/principles-4358-fixup.py"

setup() {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  command -v riot >/dev/null 2>&1 || skip "riot (Jena) not installed"
  T="$BATS_TEST_TMPDIR"
  # ownedBy names a Principal (sh:class). In the store the principals are rows in
  # the security graph; here the two owners these files use are declared beside them.
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:principal-wren a chorus:Principal .\nchorus:principal-fixture a chorus:Principal .\n' > "$T/principals.ttl"
  cat "$PC" "$T/principals.ttl" > "$T/pc.ttl"
  cat "$FIXTURE" "$T/principals.ttl" > "$T/fixture.ttl"
}

# The door (athena-model read_shape) treats a shape property with no sh:class as
# a FIELD. ownedBy always arrives as an edge to a principal, so without sh:class
# the door refused every principle write in prod, create included (2026-10-07).
# Same rule as read_shape's field query: an IRI-valued property must name its class.
iri_props_without_class() {
  arq --data "$1" --results=csv '
    PREFIX sh: <http://www.w3.org/ns/shacl#>
    PREFIX chorus: <https://jeffbridwell.com/chorus#>
    SELECT ?path WHERE { chorus:PrincipleShape sh:property ?p . ?p sh:path ?path ; sh:nodeKind sh:IRI .
                         FILTER NOT EXISTS { ?p sh:class ?c } }' | tail -n +2
}

@test "the shape file parses (riot)" {
  run riot --validate "$SHAPE"
  [ "$status" -eq 0 ]
}

@test "the 14 permaculture principle rows CONFORM to the shape" {
  run shacl validate --shapes "$SHAPE" --data "$T/pc.ttl"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'sh:conforms +true'  # simple command: a failing [[ off the last line passes on bash 3.2
  n=$(grep -c 'a chorus:Principle' "$PC")
  [ "$n" -eq 14 ]
}

# #4358 — every field the shape keeps says what it is and whether it is required.
@test "every property in PrincipleShape carries sh:name and sh:description" {
  props=$(grep -c 'sh:property \[ sh:path' "$SHAPE")
  descs=$(grep -c 'sh:description "' "$SHAPE")
  names=$(grep -c 'sh:name "' "$SHAPE")
  [ "$props" -ge 7 ]
  [ "$descs" -eq "$props" ]
  [ "$names" -eq "$props" ]
}

@test "NEGATIVE PROOF — a fixture violating every rule is REFUSED, and each violation is named" {
  run shacl validate --shapes "$SHAPE" --data "$T/fixture.ttl"
  echo "$output" | grep -qE 'sh:conforms +false'
  for needle in \
    "a principle without Jeff's reading is a quotation" \
    "a principle without its technical reading" \
    "cite as: Hemenway, T. Gaia's Garden, 2nd ed., p. N." \
    "a principle row is named hemenway-<lowercase-slug>"; do
    [[ "$output" == *"$needle"* ]] || { echo "missing violation: $needle"; return 1; }
  done
  # the missing order and the out-of-range order are two separate findings
  [ "$(echo "$output" | grep -c 'order is the Hemenway numbering, 1 to 14')" -ge 2 ]
}

@test "NEGATIVE PROOF — the fixture is not accidentally clean: fill its gaps and it conforms" {
  # the same rows with the gaps filled must pass, so a red above is the RULE firing, not a broken fixture
  python3 "$FIXUP" "$T/fixture.ttl" "$T/fixed.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/fixed.ttl"
  echo "$output" | grep -qE 'sh:conforms +true'  # simple command: a failing [[ off the last line passes on bash 3.2
}

@test "the door can accept ownedBy: every IRI property in PrincipleShape names its class" {
  run iri_props_without_class "$SHAPE"
  [ "$status" -eq 0 ]
  [ -z "$output" ] || { echo "IRI properties the door would treat as fields: $output"; return 1; }
}

@test "NEGATIVE PROOF — the same check finds ownedBy when its sh:class is removed" {
  sed 's/ sh:class chorus:Principal ;//' "$SHAPE" > "$T/no-class.ttl"
  run grep -c 'sh:class chorus:Principal' "$T/no-class.ttl"
  [ "$output" = "0" ]
  run iri_props_without_class "$T/no-class.ttl"
  [[ "$output" == *"#ownedBy"* ]] || return 1
}
