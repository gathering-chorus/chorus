#!/usr/bin/env bats
# @test-type: contract
# @domain: identity — the product domain this suite guards (#4334)
#
# #4204 reopen — Wren, 2026-10-12 08:17: "wren could PATCH Jeff's Session via
# permission-wren-identity". The door checks a row's owner, then a Permission
# on the row's graph overrides that check (athena-make permission_opens). The
# three Write grants on urn:chorus:domains:identity were added so a role could
# record its own login, but every class in that graph carries ownedBy, and an
# owned class needs no grant to create or to edit its own rows. So the grants
# only ever did one thing: let each role edit everyone else's logins, Jeff's
# included.
#
# The rule: in the identity graph a row is written by its owner and nobody else.
# Proven OFFLINE with arq over the model files: no store, no service.
# Files read:
#   roles/silas/ontology/permissions-4183.ttl
#   roles/silas/ontology/chorus.ttl  (and every ontology file naming the identity graph)
#   designing/schemas/model-retirements.jsonl

ROOT="${BATS_TEST_DIRNAME}/../.."
PERMS="$ROOT/roles/silas/ontology/permissions-4183.ttl"
LEDGER="$ROOT/designing/schemas/model-retirements.jsonl"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#>'

setup() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — this suite is UNMEASURED here, not green"
  [ -f "$PERMS" ] || skip "model files missing"
}

# Agents holding a Permission on the identity graph, one per line.
identity_grants() {
  arq --data "$1" --results=csv "$P SELECT ?a WHERE { ?p a chorus:Permission ; chorus:agent ?a ; chorus:accessTo ?g . FILTER(STR(?g) = \"urn:chorus:domains:identity\") }" 2>/dev/null | tail -n +2 | tr -d '\r' | grep . || true
}

# Classes homed in the identity graph whose shape carries no owner.
unowned_identity_classes() {
  local data=(); for f in "$@"; do data+=(--data "$f"); done
  arq "${data[@]}" --results=csv "$P SELECT ?c WHERE { ?s chorus:instancesGraph ?g ; sh:targetClass ?c . FILTER(STR(?g) = \"urn:chorus:domains:identity\") FILTER NOT EXISTS { ?s sh:property [ sh:path chorus:ownedBy ] } FILTER NOT EXISTS { ?s chorus:writeAuthority ?w } }" 2>/dev/null | tail -n +2 | tr -d '\r' | grep . || true
}

identity_model_files() { grep -l 'urn:chorus:domains:identity' "$ROOT"/roles/*/ontology/*.ttl; }

@test "no Permission grants Write on the identity graph, so no role can edit another's login" {
  got="$(identity_grants "$PERMS")"
  if [ -n "$got" ]; then echo "identity grants still held by: $got"; return 1; fi
}

@test "NEGATIVE PROOF: a model that still holds one identity grant fails the check" {
  bad="$BATS_TEST_TMPDIR/perms.ttl"
  { cat "$PERMS"; printf '\nchorus:permission-wren-identity a chorus:Permission ; chorus:agent chorus:principal-wren ; chorus:accessTo "urn:chorus:domains:identity"^^<http://www.w3.org/2001/XMLSchema#anyURI> ; chorus:mode <http://www.w3.org/ns/auth/acl#Write> .\n'; } > "$bad"
  run identity_grants "$bad"
  [ -n "$output" ]
}

@test "every class in the identity graph carries an owner, so its own role still logs in with no grant" {
  # An owned class is governed by the row's owner: create stamps the caller as
  # owner and needs no graph grant, and the caller edits its own rows. A class
  # with no owner WOULD need the grant back, and the grant is what opened Jeff's
  # rows to every role. Deploy-only classes (Principal) are written by no session.
  files=(); while IFS= read -r f; do files+=("$f"); done < <(identity_model_files)
  [ "${#files[@]}" -gt 0 ] || { echo "no model file names the identity graph"; return 1; }
  got="$(unowned_identity_classes "${files[@]}")"
  if [ -n "$got" ]; then echo "identity classes with no owner: $got"; return 1; fi
  # and the check is looking at something: the three login classes are there
  arq $(printf -- '--data %s ' "${files[@]}") --results=csv "$P ASK { chorus:SessionShape chorus:instancesGraph ?g . chorus:PresenceShape chorus:instancesGraph ?g2 . chorus:SessionRunShape chorus:instancesGraph ?g3 }" 2>/dev/null | tail -1 | grep -q true
}

@test "NEGATIVE PROOF: an identity class with no owner fails the check" {
  bad="$BATS_TEST_TMPDIR/class.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> . @prefix sh: <http://www.w3.org/ns/shacl#> .\nchorus:LooseShape a sh:NodeShape ; sh:targetClass chorus:Loose ; chorus:instancesGraph "urn:chorus:domains:identity" .\n' > "$bad"
  run unowned_identity_classes "$bad"
  [ "$output" = "https://jeffbridwell.com/chorus#Loose" ]
}

@test "the store loses the three grants too: each is retired through the ledger, scoped to the security graph" {
  for r in wren kade silas; do
    grep -q "\"retire_subject\": \"https://jeffbridwell.com/chorus#permission-$r-identity\", \"graph\": \"urn:chorus:domains:security\"" "$LEDGER" || { echo "permission-$r-identity not retired"; return 1; }
  done
}
