#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: decisions
# @card: #4710
# @owner: wren
#
# #4710 (Silas 18:33 blocker 1) — Jeff's ApprovalDecision rows are written by the lander as
# principal-lander (his passkey signature inside is the authority). The shape admits the lander
# and jeff (rows from before #4710) and refuses any other owner.
ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"

decision() {  # $1 owner local name
  cat <<TTL
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:principal-lander a chorus:Principal .
chorus:principal-jeff a chorus:Principal .
chorus:principal-kade a chorus:Principal .
chorus:approval-x a chorus:Approval .
chorus:decision-approval-x a chorus:ApprovalDecision ;
    rdfs:label "approve approval-x" ; chorus:ownedBy chorus:$1 ; chorus:decides chorus:approval-x ;
    chorus:approvalVerdict "approve" ; chorus:decidedAt "2026-11-11T23:40:00Z"^^xsd:dateTime ;
    chorus:actionSha "abc" ; chorus:passkeyProof "{}" .
TTL
}

# the ownedBy violations shacl reports for a decision owned by $1
owner_violations() {
  decision "$1" > "$BATS_TEST_TMPDIR/d.ttl"
  # one count per validation result whose focus is the decision row and whose path is ownedBy
  shacl validate --shapes "$SHAPE" --data "$BATS_TEST_TMPDIR/d.ttl" 2>&1 \
    | awk '/sh:result[ \t]+\[/{if(b ~ /decision-approval-x/ && b ~ /ownedBy/) n++; b=""} {b=b $0} END{if(b ~ /decision-approval-x/ && b ~ /ownedBy/) n++; print n+0}'
}

setup() { command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"; }

@test "a decision the lander wrote conforms on owner" {
  [ "$(owner_violations principal-lander)" -eq 0 ] || return 1
}

@test "a decision jeff wrote (before #4710) still conforms on owner" {
  [ "$(owner_violations principal-jeff)" -eq 0 ] || return 1
}

@test "negative proof: a decision a role wrote is refused on owner" {
  [ "$(owner_violations principal-kade)" -ge 1 ] || return 1
}
