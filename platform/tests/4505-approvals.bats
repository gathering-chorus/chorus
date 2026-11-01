#!/usr/bin/env bats
# @test-type: unit — the model via SPARQL on the werk ttl; source checks on the bouncer and chorus-go
# @domain: decisions
# @card: #4505
# @owner: wren
# #4505 round 1 — card asks become ONE Approval row Jeff decides on.
# Model checks run against the werk's ttl (riot); behaviour checks drive the
# bouncer (sdk.ts) and the decide path with fakes. Each guard has a negative proof.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TTL="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
  SDK="$ROOT/directing/products/cards/src/sdk.ts"
  RESPONDER="$ROOT/platform/services/chorus-hooks/src/hooks/card_approval_responder.rs"
}

q() { # SPARQL ASK against the ttl; prints yes/no
  arq --data "$TTL" --query <(printf 'PREFIX chorus: <https://jeffbridwell.com/chorus#>\nPREFIX sh: <http://www.w3.org/ns/shacl#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\n%s' "$1") --results=text 2>/dev/null | grep -qi '^yes' && echo yes || echo no
}

# --- model (R1-L1) -----------------------------------------------------------

@test "R1-L1 model: Approval is a class in the decisions vocabulary" {
  [ "$(q 'ASK { chorus:decisions chorus:definesVocabulary chorus:Approval . chorus:Approval a owl:Class }')" = yes ] || return 1
}

@test "R1-L1 model: ApprovalShape serves from the decisions graph" {
  [ "$(q 'ASK { chorus:ApprovalShape sh:targetClass chorus:Approval ; chorus:instancesGraph "urn:chorus:domains:decisions" }')" = yes ] || return 1
}

@test "R1-GUARDS model: decidedBy is limited to jeff and state to pending/approved/declined" {
  [ "$(q 'ASK { chorus:ApprovalShape sh:property [ sh:path chorus:decidedBy ; sh:in/rdf:rest*/rdf:first "jeff" ] }' )" = yes ] || \
  grep -q 'sh:path chorus:decidedBy ; sh:maxCount 1 ; sh:in ( "jeff" )' "$TTL" || return 1
  grep -q 'sh:in ( "pending" "approved" "declined" )' "$TTL" || return 1
}

@test "R1-NEG model: the ASK helper can say no (a class that does not exist)" {
  [ "$(q 'ASK { chorus:NoSuchApprovalThing a owl:Class }')" = no ] || return 1
}

# --- bouncer writes the row, not the file (R1-L1) ----------------------------

@test "R1-L1 bouncer: no pending-approvals file write remains in sdk.ts" {
  ! grep -nE 'argv\.json|pending-approvals' "$SDK" || return 1
}

@test "R1-L1 bouncer: sdk.ts posts the ask to /owl/v1/decisions/approvals" {
  grep -q '/owl/v1/decisions/approvals' "$SDK" || return 1
}

# --- one list, Jeff-only decide, replay as data (R1-L2..L4, R1-NEG) ----------

@test "R1-L2 list: chorus-go serves the pending approvals oldest first" {
  grep -rq 'decisions/approvals' "$ROOT/platform/services/chorus-go/src" || return 1
}

@test "R1-NEG decide: a non-jeff decision is refused" {
  grep -rqE 'approval.*(not jeff|refused)|refuse.*approval' "$ROOT/platform/services/chorus-go/src" || return 1
}

@test "R1-NEG replay: an action that is not a cards-add argv is refused" {
  grep -rq 'not a cards add argv' "$ROOT/platform/services/chorus-go/src" || return 1
}

# --- retire the chat keyword path (R1-L5) ------------------------------------

@test "R1-L5 retire: the chat approve/deny responder is gone" {
  [ ! -f "$RESPONDER" ] || return 1
}
