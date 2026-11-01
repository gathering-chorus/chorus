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

@test "R1-GUARDS model: a decision is its own jeff-owned row; the Approval carries no state" {
  grep -q 'sh:path chorus:ownedBy ; sh:minCount 1 ; sh:maxCount 1 ; sh:in ( chorus:principal-jeff )' "$TTL" || return 1
  grep -q 'sh:path chorus:decides ; sh:minCount 1 ; sh:maxCount 1 ; sh:class chorus:Approval' "$TTL" || return 1
  [ "$(q 'ASK { chorus:approvalState ?p ?o }')" = no ] || return 1
}

@test "R1-NEG model: the ASK helper can say no (a class that does not exist)" {
  [ "$(q 'ASK { chorus:NoSuchApprovalThing a owl:Class }')" = no ] || return 1
}

# --- bouncer writes the row, not the file (R1-L1, R1-HEADS-UP) ----------------

@test "R1-L1 bouncer: no pending-approvals file or socket write remains in sdk.ts" {
  ! grep -nE 'argv\.json|pending-approvals|bouncer-pending' "$SDK" || return 1
}

@test "R1-L1/HEADS-UP/NEG bouncer: the ask posts one row as the role; retry, bad argv, door refusal (jest)" {
  cd "$ROOT/directing/products/cards" || return 1
  run npx jest tests/approvals.test.ts
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"5 passed"* ]] || { echo "$output"; return 1; }
}

# --- one list, Jeff-only decide, replay as data (R1-L2..L4, R1-NEG) ----------

@test "R1-L2..L4/NEG chorus-go: list, passkey-only decide, replay as data, every refusal (cargo)" {
  cd "$ROOT/platform/services/chorus-go" || return 1
  run env CARGO_BUILD_JOBS=4 nice cargo test --test approvals_4505 --test serve_approvals_4505
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"test result: ok. 11 passed"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"test result: ok. 8 passed"* ]] || { echo "$output"; return 1; }
}

# --- retire the chat keyword path (R1-L5) ------------------------------------

@test "R1-L5 retire: the chat approve/deny responder and the socket handoff are gone" {
  ! grep -rnE 'bouncer.pending|card_approval_responder' "$ROOT/platform/services/chorus-hooks/src" || return 1
}

@test "R1-L5 NEG: the retire check can fail (a planted reference is seen)" {
  d="$(mktemp -d)"; echo 'route("/bouncer-pending")' > "$d/x.rs"
  grep -rqnE 'bouncer.pending|card_approval_responder' "$d" || return 1
  rm -rf "$d"
}

# --- #4642 (round 2): the go joins the one list --------------------------------

@test "R2-GO-ROW model: an Approval may be kind go as well as card, and nothing else" {
  grep -qE 'sh:path chorus:approvalKind ; sh:minCount 1 ; sh:maxCount 1 ; sh:in \( "card" "go" \)' "$TTL" || return 1
}

@test "R2-RECORDS model: a go decision records the round it signed (at most one)" {
  [ "$(q 'ASK { chorus:signedRound a owl:DatatypeProperty }')" = yes ] || return 1
  grep -q 'sh:path chorus:signedRound ; sh:maxCount 1' "$TTL" || return 1
}
