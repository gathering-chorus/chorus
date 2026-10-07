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
}

@test "the shape file parses (riot)" {
  run riot --validate "$SHAPE"
  [ "$status" -eq 0 ]
}

@test "the 14 permaculture principle rows CONFORM to the shape" {
  run shacl validate --shapes "$SHAPE" --data "$PC"
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
  run shacl validate --shapes "$SHAPE" --data "$FIXTURE"
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
  python3 "$FIXUP" "$FIXTURE" "$T/fixed.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/fixed.ttl"
  echo "$output" | grep -qE 'sh:conforms +true'  # simple command: a failing [[ off the last line passes on bash 3.2
}
