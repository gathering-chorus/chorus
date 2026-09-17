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

@test "req 6 — every write refusal names the Permission row that would open the door" {
  # three refusal sites (batch scope, entity scope, row owner) call the one namer
  n=$(grep -c 'row_that_would_open(' "$DOOR" || true)
  [ "$n" -ge 4 ]
  ! grep -q 'only the owning role may write this node' "$DOOR"
  ! grep -q 'batch requires a scoped token whose scope names' "$DOOR"
}

# --- the door writes a Permission row with its mode as a STRING (the shape types
# chorus:mode as anyURI, and the pen has no IRI-valued plain field), so the grant
# query must accept either spelling. Found live 08:24 on the fdc00b2 round: jeff's
# door-created row granted nothing. Proven offline with arq, as #4183 does.
RQ="$ROOT/platform/api/src/sparql/principal-scope.rq"
arq_rows() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — UNMEASURED here, not green"
  local q; q="$(sed 's/GRAPH <urn:chorus:domains:security> //' "$RQ")"
  printf '%s\n' "$q" > "$T/q.rq"
  arq --results csv --query "$T/q.rq" --data "$T/rows.ttl" 2>/dev/null | tail -n +2 | grep -c . || true
}
fixture() {
  T="$(mktemp -d)"
  cat > "$T/rows.ttl" <<TTL
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix acl: <http://www.w3.org/ns/auth/acl#> .
chorus:principal-jeff a chorus:Principal ; chorus:webId <https://id.example/jeff/profile/card#me> .
chorus:permission-jeff-security a chorus:Permission ; chorus:agent chorus:principal-jeff ;
  chorus:accessTo <urn:chorus:domains:security> ; chorus:mode $1 .
TTL
}

@test "a door-created row (mode stored as a string) grants through the scope query" {
  fixture '"http://www.w3.org/ns/auth/acl#Write"'
  n=$(arq_rows); rm -rf "$T"
  [ "$n" -eq 1 ]
}

@test "an authored row (mode as the acl IRI) still grants" {
  fixture 'acl:Write'
  n=$(arq_rows); rm -rf "$T"
  [ "$n" -eq 1 ]
}

@test "NEGATIVE PROOF — a Read row grants no write, in either spelling" {
  fixture 'acl:Read'; a=$(arq_rows); rm -rf "$T"
  fixture '"http://www.w3.org/ns/auth/acl#Read"'; b=$(arq_rows); rm -rf "$T"
  [ "$a" -eq 0 ]
  [ "$b" -eq 0 ]
}
