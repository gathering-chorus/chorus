#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: awareness
# @card: #4018
# @owner: wren
#
# #4018 — Pulse's own nouns. Jeff 2026-08-27: pulse is about the "shared
# awareness" case for chorus. Jeff 2026-10-10: pulse is what is happening now,
# and the Clearing mostly renders it. Four classes: a role's state (declared or
# observed), a heartbeat, a reading of one pulse section, and who is waiting on
# whom. Two proofs: good rows conform, and rows built to break each rule are
# refused with the rule named.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/awareness-4018.ttl"
MEMORY="$ROOT/roles/wren/ontology/memory-4010.ttl"

setup() {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  T="$BATS_TEST_TMPDIR"
  cat > "$T/base.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:principal-wren a chorus:Principal .
chorus:principal-kade a chorus:Principal .
chorus:card-4474 a chorus:Card .
chorus:session-kade-1 a chorus:Session .
EOF
  cat > "$T/good.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:rolestate-kade a chorus:RoleState ;
    chorus:aboutPrincipal chorus:principal-kade ; chorus:stateValue "building" ;
    chorus:stateSource "observed" ; chorus:currentCard chorus:card-4474 ;
    chorus:observedAt "2026-10-10T12:31:21Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:heartbeat-kade-1 a chorus:Heartbeat ;
    chorus:aboutPrincipal chorus:principal-kade ; chorus:inSession chorus:session-kade-1 ;
    chorus:phase "tool" ; chorus:currentTool "Bash" ;
    chorus:observedAt "2026-10-10T12:31:20Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:reading-roles a chorus:Reading ;
    chorus:section "roles" ; chorus:readFrom "pulse-worker" ;
    chorus:observedAt "2026-10-10T12:31:22Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:waitingon-wren-kade a chorus:WaitingOn ;
    chorus:waiter chorus:principal-wren ; chorus:waitsFor chorus:principal-kade ;
    chorus:waitReason "the werk-* skill rename on #4474" ; chorus:stateSource "declared" ;
    chorus:observedAt "2026-10-10T12:02:00Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
EOF
  cat > "$T/bad.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:rolestate-guess a chorus:RoleState ;
    chorus:aboutPrincipal chorus:principal-kade ; chorus:stateValue "busy" ;
    chorus:stateSource "guessed" ; chorus:ownedBy chorus:principal-wren .
chorus:reading-mood a chorus:Reading ;
    chorus:section "mood" ; chorus:readFrom "pulse-worker" ;
    chorus:observedAt "2026-10-10T12:31:22Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:waitingon-nobody a chorus:WaitingOn ;
    chorus:waitReason "waits on nothing" ; chorus:stateSource "declared" ;
    chorus:observedAt "2026-10-10T12:02:00Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
EOF
}

report() { cat "$SHAPE" "$T/base.ttl" "$1" > "$T/all.ttl"; shacl validate --shapes "$SHAPE" --data "$T/all.ttl" 2>&1; }

@test "every property in the awareness shapes is described" {
  run grep -c 'sh:path' "$SHAPE"
  paths="$output"
  run grep -c 'sh:description' "$SHAPE"
  [ "$output" -eq "$paths" ]
}

@test "the shapes claim no rule the door does not run (no sh:sparql)" {
  run grep -c 'sh:sparql' "$SHAPE"
  [ "$output" -eq 0 ]
}

@test "a role state, a heartbeat, a reading and a wait conform" {
  run report "$T/good.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || { echo "$output"; false; }
}

@test "negative proof: an unknown state, a guessed source and no time are refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"sh:conforms  false"* ]] || { echo "$output"; false; }
  [[ "$output" == *"busy"* ]] || { echo "$output"; false; }
  [[ "$output" == *"guessed"* ]] || false
  [[ "$output" == *"rolestate-guess"* && "$output" == *"observedAt"* ]] || false
}

@test "negative proof: a reading of a section pulse does not have is refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"mood"* ]] || { echo "$output"; false; }
}

@test "negative proof: a wait with no waiter and nothing waited on is refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"waitingon-nobody"* && "$output" == *"waiter"* ]] || { echo "$output"; false; }
  [[ "$output" == *"waitingon-nobody"* && "$output" == *"waitsFor"* ]] || false
}

@test "all four are short-term memory, and the awareness domain claims them" {
  for c in RoleState Heartbeat Reading WaitingOn; do
    grep -A3 "^chorus:$c a owl:Class" "$SHAPE" | grep -q 'rdfs:subClassOf chorus:ShortTermMemory' || { echo "$c is not ShortTermMemory"; false; }
    grep -A12 '^chorus:awareness a chorus:Domain' "$SHAPE" | grep -q "chorus:$c" || { echo "awareness does not claim $c"; false; }
  done
  grep -q '^chorus:ShortTermMemory a owl:Class' "$MEMORY"
}

@test "the awareness file is in the model set athena-deploy loads" {
  grep -q 'roles/wren/ontology/awareness-4018.ttl' "$ROOT/platform/services/athena-deploy/src/lib.rs"
}
