#!/usr/bin/env bats
# @test-type: unit — Jena's shacl on fixtures with the real DeliveredNeedsAPresence constraint; no store
# @domain: messages
# @card: #4915 · owner: silas
# Checks roles/silas/ontology/session-model-4302.ttl (chorus:DeliveredNeedsAPresence).
#
# #4915 — the data gate climbed with normal traffic: every alert to Jeff adds a "delivered"
# Delivery with no deliveredTo (11-21: 73 of 73 to Jeff, 10 more in the 10:20 alert burst).
# Jeff owns browser Sessions but no Presence row (measured 11:52: Presences are owned by
# wren, borg-queen, abby-normal, kade, silas only), so no writer can name one. The rule asks
# for a presence where the recipient has one to name.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  # the constraint as the model holds it (read, not retyped)
  python3 - "$ROOT/roles/silas/ontology/session-model-4302.ttl" > "$T/constraint.ttl" <<'EOF'
import re, sys
s = open(sys.argv[1]).read()
m = re.search(r'chorus:DeliveredNeedsAPresence a sh:SPARQLConstraint ;.*?""" \.', s, re.S)
print(m.group(0) if m else "")
EOF
  [ -s "$T/constraint.ttl" ] || { echo "no DeliveredNeedsAPresence in the model"; return 1; }
}

validate() { # <data turtle> -> conforms true|false
  { printf '%s\n' '@prefix sh: <http://www.w3.org/ns/shacl#> .' '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
      'chorus:DeliveryShape a sh:NodeShape ; sh:targetClass chorus:Delivery ; sh:sparql chorus:DeliveredNeedsAPresence .'
    cat "$T/constraint.ttl"; } > "$T/shapes.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n%s\n' "$1" > "$T/data.ttl"
  shacl validate --shapes "$T/shapes.ttl" --data "$T/data.ttl" 2>&1 | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'
}

JEFF='chorus:m1 chorus:sentTo chorus:principal-jeff .
chorus:s1 a chorus:Session ; chorus:ownedBy chorus:principal-jeff ; chorus:channel "browser" .
chorus:d1 a chorus:Delivery ; chorus:deliveryOf chorus:m1 ; chorus:deliveryOutcome "delivered" .'

AGENT='chorus:m1 chorus:sentTo chorus:principal-kade .
chorus:s1 a chorus:Session ; chorus:ownedBy chorus:principal-kade ; chorus:channel "pane" .
chorus:p1 a chorus:Presence ; chorus:ownedBy chorus:principal-kade .
chorus:d1 a chorus:Delivery ; chorus:deliveryOf chorus:m1 ; chorus:deliveryOutcome "delivered" .'

@test "a delivered message to a principal with sessions but no Presence (Jeff) conforms" {
  [ "$(validate "$JEFF")" = true ]
}

# NEGATIVE PROOF: a recipient with a Presence still needs it named
@test "a delivered message to a principal with a Presence and no deliveredTo fails; naming it conforms" {
  [ "$(validate "$AGENT")" = false ] || return 1
  [ "$(validate "$AGENT
chorus:d1 chorus:deliveredTo chorus:p1 .")" = true ] || return 1
}
