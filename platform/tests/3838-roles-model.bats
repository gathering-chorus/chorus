#!/usr/bin/env bats
# @test-type: unit — greps and parses TTL files in the repo plus fixtures it writes to BATS_TEST_TMPDIR; no store, no service, no network.
# #3838 — the roles model must be able to describe a role.
#
# Three defects this guards, each verified live on 2026-08-12 before the fix:
#   1. chorus:role-* existed ONLY in the live store — no source file, so a
#      deploy could not reproduce the individuals every ownership edge points at.
#   2. RoleShape did not exist, and the one shape targeting Role required
#      nothing. A Role with no label and no kind was valid.
#   3. That shape declared no instancesGraph, so owl-api resolved
#      urn:chorus:domains:roles — a graph that does not exist — and /roles/role
#      served 0 rows while reporting success.
#
# The negative proofs are the point. A test that only asserts "RoleShape exists"
# would pass against a shape that constrains nothing, which is exactly the state
# we are leaving.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  ROLES_TTL="$REPO/roles/wren/ontology/role-instances-3838.ttl"
  SHAPE_TTL="$REPO/roles/wren/ontology/priorities-3686.ttl"
  SEC_TTL="$REPO/roles/silas/ontology/security-model-3618.ttl"
  TMP="$BATS_TEST_TMPDIR"
}

# ---------------------------------------------------------------- the file ---

@test "every role is declared in a source file" {
  [ -f "$ROLES_TTL" ]
  for r in jeff wren silas kade; do
    grep -q "chorus:role-$r a chorus:Role" "$ROLES_TTL"
  done
}

@test "the source file parses" {
  command -v riot >/dev/null || skip "riot not installed"
  run riot --validate "$ROLES_TTL"
  [ "$status" -eq 0 ]
}

@test "each role carries a kind as data, not as a comment string" {
  # The store held "kind=agent" inside chorus:comment. A string in a prose field
  # cannot be constrained, queried reliably, or trusted.
  # Count roles, not a magic constant: the file had 4 roles when this was
  # written and has 5 now, so a hardcoded 4 reds on every role we add.
  n_roles=$(grep -cE '^chorus:role-[a-z-]+ a ' "$ROLES_TTL")
  run grep -c 'chorus:roleKind "' "$ROLES_TTL"
  [ "$output" -eq "$n_roles" ]
  ! grep -q 'chorus:comment "kind=' "$ROLES_TTL"
}

@test "roles are dual-typed so subclass and superclass queries both find them" {
  # Our store does no inference. A role typed only AgentRole vanishes from every
  # query asking for a Role — and every ownership constraint asks for a Role.
  grep -q "chorus:role-wren a chorus:Role, chorus:AgentRole" "$ROLES_TTL"
  grep -q "chorus:role-jeff a chorus:Role, chorus:HumanRole" "$ROLES_TTL"
}

# --------------------------------------------------------------- the shape ---

@test "RoleShape exists and requires a floor" {
  grep -q "chorus:RoleShape a sh:NodeShape" "$SHAPE_TTL"
  # Requires, not merely mentions: label and kind both minCount 1.
  run bash -c "sed -n '/chorus:RoleShape a sh:NodeShape/,/\\.$/p' '$SHAPE_TTL' | grep -c 'sh:minCount 1'"
  [ "$output" -ge 3 ]
}

@test "RoleShape pins its instances graph" {
  run bash -c "sed -n '/chorus:RoleShape a sh:NodeShape/,/\\.$/p' '$SHAPE_TTL' | grep -c 'chorus:instancesGraph \"urn:chorus:instances\"'"
  [ "$output" -eq 1 ]
}

@test "NEGATIVE PROOF: a shape without an instances-graph pin is detectable" {
  # The defect, reproduced. owl-api falls back to urn:chorus:domains:<domain>
  # when the pin is absent; for roles that graph does not exist and the route
  # serves zero rows while looking healthy. This asserts the CHECK can see the
  # bad state — without it, "RoleShape has a pin" proves nothing about whether
  # anyone would notice its removal.
  cat > "$TMP/unpinned.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh:     <http://www.w3.org/ns/shacl#> .
chorus:GhostShape a sh:NodeShape ;
  sh:targetClass chorus:Ghost ;
  sh:property [ sh:path chorus:label ; sh:minCount 1 ] .
TTL
  run bash -c "sed -n '/chorus:GhostShape a sh:NodeShape/,/\\.$/p' '$TMP/unpinned.ttl' | grep -c 'chorus:instancesGraph'"
  [ "$output" -eq 0 ]   # the check FIRES on the unpinned shape
}

# ------------------------------------------------------------- one spelling ---

@test "ownership edges point at exactly one spelling of a role" {
  cd "$REPO"
  # No ownership edge may name the retired IRIs.
  run bash -c "grep -rhoE 'chorus:(ownedBy|ownerRole|gatekeeper|assignedTo) chorus:(wren|silas|kade|jeff)\b' --include='*.ttl' . | wc -l | tr -d ' '"
  [ "$output" -eq 0 ]
}

@test "NEGATIVE PROOF: the one-spelling check can fail" {
  # A grep that matches nothing passes for two reasons — the rule holds, or the
  # pattern is wrong. Prove the pattern still finds a violation when one exists.
  cat > "$TMP/violation.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:product-x chorus:ownedBy chorus:wren .
TTL
  run bash -c "grep -hoE 'chorus:(ownedBy|ownerRole|gatekeeper|assignedTo) chorus:(wren|silas|kade|jeff)\b' '$TMP/violation.ttl' | wc -l | tr -d ' '"
  [ "$output" -eq 1 ]
}

@test "the retired spellings declare no Role individual anywhere" {
  cd "$REPO"
  # Comment lines are stripped first: this card DOCUMENTS the retired names in
  # role-instances-3838.ttl so the next reader knows what was collapsed and why.
  # A check that cannot tell prose from a declaration would make recording the
  # decision impossible — and an undocumented retirement is how someone
  # reintroduces the spelling in six weeks.
  run bash -c "grep -rh --include='*.ttl' -v '^[[:space:]]*#' . | grep -cE 'chorus:(wren|silas|kade|jeff) a chorus:Role\\b|chorus:(wren|silas)-owner' || true"
  [ "$output" -eq 0 ]
}

@test "NEGATIVE PROOF: the retired-spelling check still sees a real declaration" {
  # Stripping comments must not blind the check to an actual re-introduction.
  cat > "$TMP/reintroduced.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:wren a chorus:Role ; rdfs:label "Wren" .
TTL
  run bash -c "grep -h -v '^[[:space:]]*#' '$TMP/reintroduced.ttl' | grep -cE 'chorus:(wren|silas|kade|jeff) a chorus:Role\\b'"
  [ "$output" -eq 1 ]
}

# ------------------------------------------------------------------- seam ---

@test "holdsRole is declared in PrincipalShape so the API can project it" {
  # It was live in the store and absent from the shape, so /principals never
  # returned it: the identity-to-role seam existed and was invisible.
  grep -q "sh:property chorus:PrincipalShape-holdsRole" "$SEC_TTL"
  grep -q "chorus:PrincipalShape-holdsRole a sh:PropertyShape" "$SEC_TTL"
}

@test "holdsRole is typed, so a dangling role reference is refused" {
  run bash -c "sed -n '/chorus:PrincipalShape-holdsRole a sh:PropertyShape/,/\\.$/p' '$SEC_TTL' | grep -c 'sh:class chorus:Role'"
  [ "$output" -eq 1 ]
}

@test "the WebID correlation key is unique, expressed in the idiom that RUNS" {
  # It was a bare string with no uniqueness rule — two principals could claim the
  # same identity and nothing would object.
  #
  # This asserts chorus:uniqueGlobal, NOT a sh:sparql shape. I wrote it as
  # sh:sparql first; the chorus-model validator never reads sh:sparql (zero
  # occurrences in the crate), so it would have looked enforced and never fired.
  # The behavioural proof lives in the crate — webid_uniqueness_3838 — where a
  # duplicate is actually refused. This grep only pins the DECLARATION.
  # Block delimited by the next BLANK LINE, not by a trailing period: the
  # comment explaining this choice ends in a period, which truncated the sed
  # range and failed against a file that was correct. A range ending in the
  # wrong place is its own small hollow gate.
  #
  # And the match is anchored to the start of the line so the comment that
  # NAMES uniqueGlobal cannot satisfy the check — prose about a rule is not the
  # rule, which is the whole lesson of this card.
  run bash -c "awk '/PrincipalShape-webId a sh:PropertyShape/,/^[[:space:]]*\$/' '$SEC_TTL' | grep -cE '^[[:space:]]+chorus:uniqueGlobal true'"
  [ "$output" -eq 1 ]
}

@test "NEGATIVE PROOF: the decorative sh:sparql rule is GONE, not left beside the real one" {
  # Two rules for one thing is how three spellings of a role happened. The
  # inert one had to be deleted, not demoted.
  ! grep -q "chorus:PrincipalWebIdUnique" "$SEC_TTL"
}

# ------------------------------------------------------- the word cap ---

@test "the response word cap is registered as a property key, not a constant" {
  grep -q "chorus:pk-responseWordCap a chorus:PropertyKey" "$SHAPE_TTL"
  grep -q 'chorus:keyName "response.word.cap"' "$SHAPE_TTL"
  grep -q "chorus:appliesToClass chorus:Role" "$SHAPE_TTL"
}
