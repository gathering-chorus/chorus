#!/usr/bin/env bats
# @test-type: contract
# @domain: security — the product domain this suite guards (#4334)
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

# #4336: these cases grepped the door's and the OIDC server's Rust source for
# function names and strings. They now RUN the crates' own unit tests for the
# behaviour, by exact name, so a renamed or deleted test fails here loudly
# instead of a grep quietly still matching a comment.
crate_test() {  # crate_test <crate> <exact test name>
  command -v cargo >/dev/null 2>&1 || skip "UNMEASURED — cargo absent, cannot run the $1 unit test (#4336)"
  run bash -c "cd '$ROOT/platform/services/$1' && cargo test --release -q -- --exact '$2' 2>&1"
  [ "$status" -eq 0 ]
  [[ "$output" == *"1 passed"* ]] || { echo "$1: $2 did not run as exactly one passing test"; echo "$output" | tail -5; return 1; }
}

@test "the door stamps the verified caller, not a body-supplied owner" {
  crate_test athena-make bounds_closedshape_tests::the_door_stamps_the_write_and_refuses_a_body_stamp
  crate_test athena-make bounds_closedshape_tests::verified_owner_uses_the_shape_declared_edge_and_ignores_body_owner
}

@test "NEGATIVE PROOF — the old spellings of one owner compare equal; two users never do" {
  crate_test athena-make tests::owner_is_a_principal_and_the_old_spellings_still_name_the_same_user
}

@test "the caller's name comes from the graph rows, and no resolver yields no name" {
  crate_test chorus-oidc oidc::tests::principal_names_resolve_from_rows_and_a_missing_resolver_yields_no_name
}

@test "model — ownedBy ranges over Principal, and no owner shape still says Role" {
  command -v arq >/dev/null 2>&1 || skip "UNMEASURED — arq absent (#4336)"
  T="$BATS_TEST_TMPDIR"
  printf '%s\n' 'PREFIX chorus: <https://jeffbridwell.com/chorus#>' 'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>' \
    'ASK { chorus:ownedBy rdfs:range chorus:Principal FILTER NOT EXISTS { chorus:ownedBy rdfs:range chorus:Role } }' > "$T/range.rq"
  run arq --data "$MODEL" --query "$T/range.rq"
  [ "$output" = "yes" ]
  printf '%s\n' 'PREFIX chorus: <https://jeffbridwell.com/chorus#>' 'PREFIX sh: <http://www.w3.org/ns/shacl#>' \
    'SELECT (COUNT(?ps) AS ?n) WHERE { ?ps sh:path chorus:ownedBy ; sh:class chorus:Role }' > "$T/shapes.rq"
  run arq --results=csv --data "$MODEL" --data "$ROOT/roles/wren/ontology/board-3654.ttl" \
    --data "$ROOT/roles/kade/ontology/domains-kade-3581.ttl" --query "$T/shapes.rq"
  [ "$(printf '%s\n' "$output" | tail -1 | tr -d '\r')" = "0" ]
}

@test "NEGATIVE PROOF — a fixture with an ownedBy shape bound to Role is counted by the same query" {
  command -v arq >/dev/null 2>&1 || skip "UNMEASURED — arq absent (#4336)"
  T="$BATS_TEST_TMPDIR"
  printf '%s\n' '@prefix chorus: <https://jeffbridwell.com/chorus#> .' '@prefix sh: <http://www.w3.org/ns/shacl#> .' \
    'chorus:BadShape sh:property [ sh:path chorus:ownedBy ; sh:class chorus:Role ] .' > "$T/bad.ttl"
  printf '%s\n' 'PREFIX chorus: <https://jeffbridwell.com/chorus#>' 'PREFIX sh: <http://www.w3.org/ns/shacl#>' \
    'SELECT (COUNT(?ps) AS ?n) WHERE { ?ps sh:path chorus:ownedBy ; sh:class chorus:Role }' > "$T/shapes.rq"
  run arq --results=csv --data "$T/bad.ttl" --query "$T/shapes.rq"
  [ "$(printf '%s\n' "$output" | tail -1 | tr -d '\r')" = "1" ]
}

@test "req 6 — a write refusal names the Permission row that would open the door" {
  crate_test athena-make tests::a_refusal_names_the_row_that_would_open_the_door
}

# --- the door writes a Permission row with its mode as a STRING (the shape types
# chorus:mode as anyURI, and the pen has no IRI-valued plain field), so the grant
# query must accept either spelling. Found live 08:24 on the fdc00b2 round: jeff's
# door-created row granted nothing. Proven offline with arq, as #4183 does.
RQ="$ROOT/platform/api/src/sparql/principal-scope.rq"
arq_rows() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — UNMEASURED here, not green"
  # #4256 — the query names TWO graphs: Permissions in the security graph and
  # Principals in the urn:chorus:principal-home MARKER, which server.ts:110
  # substitutes at runtime from PrincipalShape's instancesGraph and which
  # exists nowhere in the store. Stripping only the first left the marker
  # standing, so arq matched nothing and all three cases below returned 0 —
  # including the NEGATIVE PROOF, which passed for the wrong reason.
  local q; q="$(sed -e 's/GRAPH <urn:chorus:domains:security> //' \
                    -e 's/GRAPH <urn:chorus:principal-home> //' "$RQ")"
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

@test "the negative proof below cannot pass vacuously — a Write row grants 1" {
  # #4256: tests 8, 9 and 10 all returned 0 for a day because an unsubstituted
  # graph marker made arq match nothing, and 10 read that as "correctly denied".
  # This asserts the query can answer at all, so a zero in 10 means Read.
  fixture 'acl:Write'; n=$(arq_rows); rm -rf "$T"
  [ "$n" -eq 1 ]
}

@test "NEGATIVE PROOF — a Read row grants no write, in either spelling" {
  fixture 'acl:Read'; a=$(arq_rows); rm -rf "$T"
  fixture '"http://www.w3.org/ns/auth/acl#Read"'; b=$(arq_rows); rm -rf "$T"
  [ "$a" -eq 0 ]
  [ "$b" -eq 0 ]
}
