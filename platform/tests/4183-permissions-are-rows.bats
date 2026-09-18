#!/usr/bin/env bats
# @test-type: contract
# #4183 — permissions are ROWS (acl:Authorization), and the door's scope query
# reads rows, not hasScope literals. Proven OFFLINE with Jena's arq over the
# model files themselves: no store, no service, no prod write. The negative
# proof is the one that matters: a file holding ONLY hasScope literals grants
# nothing through the new query.

ROOT="${BATS_TEST_DIRNAME}/../.."
RQ="$ROOT/platform/api/src/sparql/principal-scope.rq"
PERMS="$ROOT/roles/silas/ontology/permissions-4183.ttl"
PRINCIPALS="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
OLD_SCOPES="$ROOT/roles/silas/ontology/security-scopes-3689.ttl"

setup() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — this suite is UNMEASURED here, not green"
  [ -f "$RQ" ] && [ -f "$PERMS" ] && [ -f "$PRINCIPALS" ] || skip "model files missing"
  # the .rq names the security GRAPH; offline the files are the default graph
  Q="$(sed 's/GRAPH <urn:chorus:domains:security> //' "$RQ")"
  T="$(mktemp -d)"; printf '%s\n' "$Q" > "$T/q.rq"
}
teardown() { rm -rf "$T"; }

rows() { arq --results csv --query "$T/q.rq" "$@" 2>/dev/null | tail -n +2 | grep -c . || true; }

@test "the permissions file holds every grant it replaced, and the three #4204 added" {
  # 42 was the hasScope set this file replaced. #4204 (2026-09-18) moved Session rows
  # out of the security graph into identity and opened that graph to the three roles,
  # which is three more rows — not drift. The count is asserted, not floored, so a row
  # vanishing still goes red.
  run grep -c "a chorus:Permission" "$PERMS"
  test "$output" -eq 45
}

@test "the scope query grants from Permission rows joined to real principals" {
  n=$(rows --data "$PERMS" --data "$PRINCIPALS")
  [ "$n" -ge 30 ] || { echo "only $n grants resolved"; false; }
}

@test "NEGATIVE PROOF — hasScope literals alone grant NOTHING through the query" {
  [ -f "$OLD_SCOPES" ] || skip "old scopes file already retired"
  n=$(rows --data "$OLD_SCOPES" --data "$PRINCIPALS")
  [ "$n" -eq 0 ] || { echo "hasScope still grants $n — the query did not move"; false; }
}

@test "NEGATIVE PROOF — a Read-mode row is not a write grant" {
  # nudge-read rows are mode acl:Read; none may appear as a write scope
  arq --results csv --query "$T/q.rq" --data "$PERMS" --data "$PRINCIPALS" 2>/dev/null | grep -q "nudge-read" && {
    echo "a Read row came back as a write grant"; false; }
  true
}

@test "NEGATIVE PROOF — a row whose agent is not a Principal grants nothing" {
  cat > "$T/ghost.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix acl:    <http://www.w3.org/ns/auth/acl#> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .
chorus:permission-ghost a chorus:Permission ;
    chorus:agent chorus:principal-ghost ;
    chorus:accessTo "urn:chorus:domains:security"^^xsd:anyURI ;
    chorus:mode acl:Write .
TTL
  n=$(rows --data "$T/ghost.ttl")
  [ "$n" -eq 0 ]
}

@test "return gate — the query no longer mentions hasScope, and reads acl:accessTo" {
  ! grep -q "hasScope" "$RQ"
  grep -q "chorus:accessTo" "$RQ"
  grep -q "acl:Write" "$RQ"
}

@test "return gate — the security deploy set carries the rows file and not the literals file" {
  D="$ROOT/platform/scripts/athena-deploy-model.sh"
  grep -q 'permissions-4183.ttl' "$D"
  ! grep -E '^\s*"\$CHORUS_ROOT/roles/silas/ontology/security-scopes-3689.ttl"' "$D"
}
