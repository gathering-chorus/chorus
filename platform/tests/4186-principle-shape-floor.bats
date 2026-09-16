#!/usr/bin/env bats
# @test-type: unit — validates TTL files with Jena's shacl CLI; no store, no network.
#
# #4186 — Jeff, 2026-09-16: "use principles, the class and shacl is shallow". The
# shape now requires what every live row already carries (both readings, source,
# owner) and binds the Hemenway numbering to the 14 roots. Two proofs: the real
# rows conform, and a fixture built to violate every new rule is REFUSED, each
# violation named. The land of this card is also the first run of athena.yml's
# land job for real, since principles-3749.ttl is in the model set.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/principles-3749.ttl"
PC="$ROOT/roles/wren/ontology/principles-instances-3749.ttl"
XP="$ROOT/roles/wren/ontology/principles-xp-4006.ttl"
ROLES="$ROOT/roles/wren/ontology/role-instances-3838.ttl"
FIXTURE="$ROOT/platform/tests/fixtures/principles-4186-violations.ttl"

setup() {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  command -v riot >/dev/null 2>&1 || skip "riot (Jena) not installed"
  T="$BATS_TEST_TMPDIR"
}

@test "the shape file parses (riot)" {
  run riot --validate "$SHAPE"
  [ "$status" -eq 0 ]
}

@test "the 28 live principle rows CONFORM to the deepened shape (14 pc + 14 xp, roles present)" {
  riot --output=ttl "$PC" "$XP" "$ROLES" > "$T/data.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/data.ttl"
  [ "$status" -eq 0 ]
  [[ "$output" == *"sh:conforms  true"* ]] || [[ "$output" == *"sh:conforms true"* ]]
  n=$(grep -c 'a chorus:Principle' "$PC" "$XP" | awk -F: '{s+=$2} END{print s}')
  [ "$n" -eq 28 ]
}

@test "NEGATIVE PROOF — a fixture violating every new rule is REFUSED, and each violation is named" {
  run shacl validate --shapes "$SHAPE" --data "$FIXTURE"
  [[ "$output" == *"sh:conforms  false"* ]] || [[ "$output" == *"sh:conforms false"* ]]
  for needle in \
    "a principle without Jeff's reading is a quotation" \
    "a principle without its technical reading" \
    "every principle names where it came from" \
    "a permaculture root carries its Hemenway number" \
    "isPermacultureParent true" \
    "only the 14 permaculture roots are numbered" \
    "order is the Hemenway numbering, 1 to 14"; do
    [[ "$output" == *"$needle"* ]] || { echo "missing violation: $needle"; return 1; }
  done
}

@test "NEGATIVE PROOF — the fixture is not accidentally clean: strip its violations and it conforms" {
  # the same rows with the gaps filled must pass, so a red above is the RULE firing, not a broken fixture
  python3 - "$FIXTURE" "$T/fixed.ttl" <<'PY'
import sys,re
t=open(sys.argv[1]).read()
t=t.replace('chorus:principleKind "pc" ; chorus:order 1 ; chorus:isPermacultureParent true .',
 'chorus:principleKind "pc" ; chorus:order 1 ; chorus:isPermacultureParent true ;\n    dcterms:source "Hemenway p.6" ; chorus:techReading "Watch a role work before directing it, read the board first." ; chorus:jeffReading "Jeff reads the board and listens before committing to a move." .')
t=t.replace('chorus:principleKind "pc" .','chorus:principleKind "pc" ; chorus:order 2 ; chorus:isPermacultureParent true .')
t=t.replace('chorus:principleKind "xp" ; chorus:order 3 .','chorus:principleKind "xp" .')
t=t.replace('chorus:principleKind "zen" ; chorus:order 15 .','chorus:principleKind "pc" ; chorus:order 14 ; chorus:isPermacultureParent true .')
open(sys.argv[2],'w').write(t)
PY
  run shacl validate --shapes "$SHAPE" --data "$T/fixed.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || [[ "$output" == *"sh:conforms true"* ]]
}
