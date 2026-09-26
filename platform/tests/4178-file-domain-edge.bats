#!/usr/bin/env bats
# @test-type: contract
# @domain: code — the product domain this suite guards (#4334)
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
# check that cannot reach it proves nothing.
#
# #4265 — this used chorus:SubDomain as the fixture class. SubDomain was RETIRED
# (#4216/#4237) and the grep for its declaration started failing, so the proof
# went red for the one reason a guard must never go red: its fixture was deleted
# out from under it, not the rule it guards. chorus:Vertebra is the replacement
# and is checked for BOTH properties the fixture needs — declared in the
# ontology, absent from the DAL kind table — so the day Vertebra is retired or
# becomes mintable this test says so instead of quietly meaning nothing.
@test "NEGATIVE PROOF: an edge ranged on an unmintable class is caught" {
  grep -q '^chorus:Vertebra a owl:Class' "$TTL" \
    || { echo "fixture class chorus:Vertebra is no longer declared — pick another, do not delete this proof" >&2; false; }
  run grep -E '\("vertebra", *"Vertebra"' "$KINDS"
  [ "$status" -ne 0 ] \
    || { echo "chorus:Vertebra became mintable — this fixture no longer represents an unmintable class" >&2; false; }

  fixture="$BATS_TEST_TMPDIR/fixture.ttl"
  printf 'chorus:fileInDomain a owl:ObjectProperty ;\n    rdfs:range chorus:Vertebra ;\n' > "$fixture"
  run grep -A 5 '^chorus:fileInDomain a owl:ObjectProperty' "$fixture"
  [[ "$output" != *"rdfs:range chorus:Domain"* ]]
}
