#!/usr/bin/env bats
# @test-type: contract
# #4196 — one authz, step 2. A permission is a row, a role is a hat, an owner
# is a user. These are the return gates: the door's write paths no longer ask
# for a hat, and the model no longer says an owner is a Role. The live proofs
# (jeff, who holds a permission and wears no hat, creates and deletes a row
# through the door) run at demo against the variant, by hand, and are on the
# card; they cannot run here because needs-stack tests hit the live stack,
# which serves the OLD door until this card lands.

ROOT="${BATS_TEST_DIRNAME}/../.."
DOOR="$ROOT/platform/services/athena-make/src/lib.rs"
OIDC="$ROOT/platform/services/chorus-oidc/src/oidc.rs"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"

@test "the door's write path no longer gates on a resolved role" {
  # v1 had two call sites (entity writes and /batch). Zero means no hat is asked.
  n=$(grep -c 'resolved_write_role(&claims' "$DOOR" || true)
  [ "$n" -eq 0 ]
}

@test "the door stamps and compares the caller by principal name, not hat" {
  grep -q 'principal_for(&claims.web_id' "$DOOR"
  grep -q 'with_principal_names' "$DOOR"
}

@test "NEGATIVE PROOF — the old spellings of one owner compare equal; two users never do" {
  # the pure comparison is unit-tested in the crate; this proves the test exists and names both directions
  grep -q 'owner_is_a_principal_and_the_old_spellings_still_name_the_same_user' "$DOOR"
  grep -q 'principal-silas' "$DOOR"
}

@test "the caller's name comes from the graph, not parsed out of the WebID" {
  grep -q 'fn principal_name_query' "$OIDC"
  grep -q 'REPLACE(REPLACE(STR(?p)' "$OIDC"
  ! grep -qE 'web_id\.(split|rsplit|trim)\(.*profile' "$OIDC"
}

@test "model — ownedBy ranges over Principal, and no owner shape still says Role" {
  grep -A9 '^chorus:ownedBy a owl:ObjectProperty' "$MODEL" | grep -q 'rdfs:range chorus:Principal'
  ! grep -A9 '^chorus:ownedBy a owl:ObjectProperty' "$MODEL" | grep -q 'rdfs:range chorus:Role'
  # every property shape whose path is ownedBy: sh:class must be Principal
  for f in "$MODEL" "$ROOT/roles/wren/ontology/board-3654.ttl" "$ROOT/roles/kade/ontology/domains-kade-3581.ttl"; do
    bad=$(tr '\n' ' ' < "$f" | grep -oE '\[[^]]*sh:path chorus:ownedBy[^]]*\]' | grep -c 'sh:class chorus:Role' || true)
    [ "$bad" -eq 0 ] || { echo "$f still binds an ownedBy shape to Role"; false; }
  done
}

@test "NEGATIVE PROOF — a fixture with an ownedBy shape bound to Role is caught by the model gate" {
  T="$(mktemp -d)"
  printf 'x:S a sh:NodeShape ; sh:property [ sh:path chorus:ownedBy ; sh:class chorus:Role ] .\n' > "$T/bad.ttl"
  bad=$(tr '\n' ' ' < "$T/bad.ttl" | grep -oE '\[[^]]*sh:path chorus:ownedBy[^]]*\]' | grep -c 'sh:class chorus:Role' || true)
  rm -rf "$T"
  [ "$bad" -eq 1 ]
}
