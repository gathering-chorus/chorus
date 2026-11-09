#!/usr/bin/env bats
# @test-type: unit
# @domain: werk
# #4773 AC1: Define DeploymentManifest / Deployment shape in OWL
# (chorus:DeploymentShape) with cardId, commitSha, sha256, targetMachine,
# deployedBy, deployedAt, previous, rollbackOf.
# Negative proof: a Deployment row without cdhash is refused.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MODEL="$ROOT/roles/kade/ontology/werk-domains.ttl"
  FIX="$ROOT/platform/tests/fixtures/manifest-4773"
  SHAPE="$BATS_FILE_TMPDIR/shape.ttl"

  # Extract prefixes and chorus:DeploymentShape from werk-domains.ttl
  { grep '^@prefix' "$MODEL"
    awk '/^chorus:DeploymentShape a sh:NodeShape/{p=1} p{print} p && /\. *$/ && !/^ *#/{exit}' "$MODEL"; } > "$SHAPE"
  export SHAPE FIX MODEL
}

conforms() {
  shacl validate --shapes "$SHAPE" --data "$1" 2>&1 | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'
}

@test "4773 AC1: DeploymentShape targets chorus:Deployment in model" {
  grep -q 'sh:targetClass chorus:Deployment' "$SHAPE"
}

@test "4773 AC1: DeploymentShape declares required edges and properties" {
  # Required cdhash (minCount 1, maxCount 1)
  grep -A 2 'sh:path chorus:cdhash' "$SHAPE" | grep -q 'sh:minCount 1'
  grep -A 2 'sh:path chorus:cdhash' "$SHAPE" | grep -q 'sh:maxCount 1'
  # Declares previous edge
  grep -q 'sh:path chorus:previous' "$SHAPE"
  # Declares rollbackOf edge
  grep -q 'sh:path chorus:rollbackOf' "$SHAPE"
  # Declares targetMachine
  grep -q 'sh:path chorus:targetMachine' "$SHAPE"
  # Declares deployedBy pointing at chorus:Principal
  grep -A 2 'sh:path chorus:deployedBy' "$SHAPE" | grep -q 'sh:class chorus:Principal'
  # Declares cardId, deployedCommit, sha256, deployedAt with xsd:dateTime
  grep -q 'sh:path chorus:cardId' "$SHAPE"
  grep -q 'sh:path chorus:deployedCommit' "$SHAPE"
  grep -q 'sh:path chorus:sha256' "$SHAPE"
  grep -A 2 'sh:path chorus:deployedAt' "$SHAPE" | grep -q 'sh:datatype xsd:dateTime'
}

@test "4773 AC1: valid Deployment conforming fixture passes SHACL validation" {
  [ "$(conforms "$FIX/good.ttl")" = "true" ]
}

@test "4773 AC1 NEGATIVE PROOF: Deployment row without cdhash is refused" {
  [ "$(conforms "$FIX/bad-no-cdhash.ttl")" = "false" ]
}

@test "4773 AC1 NEGATIVE PROOF: deployedBy pointing at a non-Principal is refused" {
  [ "$(conforms "$FIX/bad-deployedby-non-principal.ttl")" = "false" ]
}
