#!/usr/bin/env bats
# @test-type: integration
# #3618 — two test surfaces, per DEC-1674 red-first:
#   A. SCRIPT SHAPE (runs pre-apply): security-3618-migrate.sh generate emits a
#      well-formed door batch — 4-field lines, DEL sweep covers security-trust,
#      INS carries the renamed chorus:security family, and NO INS line
#      resurrects the retired name. RED until the script exists and is correct.
#   B. DONE-STATE (live graph): RED until the migration is applied through the
#      door; GREEN is the definition of done for the model AC. Same pattern as
#      products-3603-migration.bats.

NS="https://jeffbridwell.com/chorus#"
EP="http://localhost:3030/pods/sparql"
REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/security-3618-migrate.sh"

ask() { # $1 = WHERE body -> prints "True"/"False"
  curl -s --max-time 10 "$EP" --data-urlencode "query=PREFIX chorus: <$NS> PREFIX sh: <http://www.w3.org/ns/shacl#> ASK { GRAPH ?g { $1 } }" \
    -H "Accept: application/sparql-results+json" | python3 -c "import sys,json;print(json.load(sys.stdin)['boolean'])"
}

# --- A. script shape (pre-apply, red until the script is right) ---

@test "generate emits only well-formed 4-field OP lines" {
  [ -x "$SCRIPT" ]
  run bash -c "'$SCRIPT' generate 2>/dev/null | awk -F'\t' 'NF!=4 || (\$1!=\"DEL\" && \$1!=\"INS\")' | wc -l"
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tr -d ' ')" -eq 0 ]
}

@test "generate DEL sweep covers the retired security-trust node (out + in)" {
  # PRE-APPLY shape check: DEL is live-derived, so once the migration has been
  # applied and security-trust is gone, generate correctly produces zero DELs
  # (idempotency). Skip post-apply rather than false-fail — the done-state tests
  # below are what prove the migration landed.
  if [ "$(ask 'chorus:security-trust ?p ?o')" = "False" ] && [ "$(ask '?s ?p chorus:security-trust')" = "False" ]; then
    skip "already applied — security-trust gone, DEL sweep correctly empty (idempotent)"
  fi
  run bash -c "'$SCRIPT' generate 2>/dev/null | grep -c $'^DEL\t.*security-trust'"
  [ "$(echo "$output" | tr -d ' ')" -ge 2 ]
}

@test "generate INS carries the renamed security domain node with the #3466 floor" {
  body=$("$SCRIPT" generate 2>/dev/null)
  echo "$body" | grep -q $'^INS\t<'"${NS}"$'security>\t.*instancesGraph' || \
    echo "$body" | grep $'^INS\t<'"${NS}"$'security>' | grep -q "instancesGraph"
  echo "$body" | grep $'^INS\t<'"${NS}"$'security>' | grep -q "ownedBy"
  echo "$body" | grep $'^INS\t<'"${NS}"$'security>' | grep -q "partOf"
}

@test "generate INS carries the identity child + Principal vocabulary" {
  body=$("$SCRIPT" generate 2>/dev/null)
  echo "$body" | grep $'^INS\t<'"${NS}"$'identity>' | grep -q "partOf.*${NS}security"
  echo "$body" | grep -q $'^INS\t<'"${NS}"$'Principal>'
  echo "$body" | grep -q $'^INS\t<'"${NS}"$'holdsRole>'
}

@test "no INS line resurrects the retired name" {
  run bash -c "'$SCRIPT' generate 2>/dev/null | grep -c $'^INS\t.*security-trust'"
  [ "$(echo "$output" | tr -d ' ')" -eq 0 ]
}

# --- B. done-state (live graph; RED until applied through the door) ---

@test "live: chorus:security exists as a Domain with instancesGraph" {
  [ "$(ask 'chorus:security a chorus:Domain ; chorus:instancesGraph ?g2')" = "True" ]
}

@test "live: chorus:security-trust is gone (no triples in or out)" {
  [ "$(ask 'chorus:security-trust ?p ?o')" = "False" ]
  [ "$(ask '?s ?p chorus:security-trust')" = "False" ]
}

@test "live: identity is a child domain of security" {
  [ "$(ask 'chorus:identity a chorus:Domain ; chorus:partOf chorus:security')" = "True" ]
}

@test "live: Principal class and PrincipalShape floor exist" {
  [ "$(ask 'chorus:Principal a ?t')" = "True" ]
  [ "$(ask 'chorus:PrincipalShape sh:targetClass chorus:Principal')" = "True" ]
}

@test "live: borgProduct hasDomain repointed to security" {
  [ "$(ask 'chorus:borgProduct chorus:hasDomain chorus:security')" = "True" ]
}
