#!/usr/bin/env bats
# @test-type: contract
# #4178 — a file's domain edge must point at a class the DAL can mint.
#
# fileInDomain ranged on chorus:SubDomain, the class Jeff retired 2026-06-19.
# athena-model's mint allowlist never carried it, so every attempt to tag a file
# answered 502 unknown-kind: 'sub-domain'. 0 of 5542 rows carried a domain — not
# because nobody wrote one, but because nobody could, and the team read that
# absence twice, in writing, as evidence the class was dead.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TTL="$REPO/roles/silas/ontology/chorus.ttl"
  KINDS="$REPO/platform/services/athena-model/src/lib.rs"
}

@test "the file domain edge points at Domain, in both the property and the shape" {
  run grep -A 5 '^chorus:fileInDomain a owl:ObjectProperty' "$TTL"
  [[ "$output" == *"rdfs:range chorus:Domain"* ]]
  run grep 'sh:path chorus:fileInDomain' "$TTL"
  [[ "$output" == *"sh:class chorus:Domain"* ]]
}

@test "Domain is a kind the DAL can mint" {
  run grep -E '\("domain", *"Domain"' "$KINDS"
  [ "$status" -eq 0 ]
}

# NEGATIVE PROOF (#3734): the check must fail when the edge points at a class the
# mint allowlist does not carry — that is the exact state this card found, and a
# check that cannot reach it proves nothing. SubDomain is the real example: it is
# declared in the ontology and absent from the allowlist.
@test "NEGATIVE PROOF: an edge ranged on an unmintable class is caught" {
  grep -q '^chorus:SubDomain a owl:Class' "$TTL"
  run grep -E '\("sub-domain", *"SubDomain"' "$KINDS"
  [ "$status" -ne 0 ]

  fixture="$BATS_TEST_TMPDIR/fixture.ttl"
  printf 'chorus:fileInDomain a owl:ObjectProperty ;\n    rdfs:range chorus:SubDomain ;\n' > "$fixture"
  run grep -A 5 '^chorus:fileInDomain a owl:ObjectProperty' "$fixture"
  [[ "$output" != *"rdfs:range chorus:Domain"* ]]
}
