#!/usr/bin/env bats
# @test-type: contract
# @domain: security — the product domain this suite guards (#4334)
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
  # The .rq names its graphs; offline the files are the default graph, so every
  # GRAPH clause has to come off. #4224 split principals into their own graph
  # (urn:chorus:principal-home) and this stripped only the security one, so the
  # second clause matched nothing and the query answered 0 for every grant —
  # a harness that could no longer see what it was asserting.
  Q="$(sed -E 's/GRAPH <urn:chorus:[a-z:-]+> //g' "$RQ")"
  T="$(mktemp -d)"; printf '%s\n' "$Q" > "$T/q.rq"
}
teardown() { rm -rf "$T"; }

rows() { arq --results csv --query "$T/q.rq" "$@" 2>/dev/null | tail -n +2 | grep -c . || true; }

@test "the permissions file holds every grant it replaced, and the three #4204 added" {
  # 42 was the hasScope set this file replaced. #4204 (2026-09-18) moved Session rows
  # out of the security graph into identity and opened that graph to the three roles,
  # which is three more rows — not drift. The count is asserted, not floored, so a row
  # vanishing still goes red.
  #
  # 47 since #4229 (2026-09-20): permission-kade-pipelines, on Jeff's go — he
  # owns 120 of that graph's 130 rows and could not write it, so the live
  # PipelineRun check 403'd for everyone.
  # 46 since #4222 (2026-09-20): permission-kade-logs. The crawler writes LogSource
  # and runs as kade, so under Jeff's 09-17 ruling those rows are kade's and the
  # grant was the missing member of the set — 133 PUTs were 403ing without it.
  # Named here on purpose: bumping this number is a decision, not a rubber stamp.
  run grep -c "a chorus:Permission" "$PERMS"
  test "$output" -eq 47
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
  # #4229 - the security set is a manifest row now, not a bash array.
  MAN="$ROOT/platform/config/domain-set-manifest.txt"
  grep -q "permissions-4183.ttl" "$MAN"
  # NEGATIVE PROOF: the literals file must not have come back with it.
  test -z "$(grep -F "security-scopes-3689.ttl" "$MAN" || true)"
}
