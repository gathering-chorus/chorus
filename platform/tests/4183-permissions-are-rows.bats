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
  # 48 since #3102 (2026-10-02): permission-jeff-cards, on Jeff's go. The land's
  # accept step runs `cards done` as jeff, and the graph write-through 403'd.
  # 47 since #4229 (2026-09-20): permission-kade-pipelines, on Jeff's go — he
  # owns 120 of that graph's 130 rows and could not write it, so the live
  # PipelineRun check 403'd for everyone.
  # 46 since #4222 (2026-09-20): permission-kade-logs. The crawler writes LogSource
  # and runs as kade, so under Jeff's 09-17 ruling those rows are kade's and the
  # grant was the missing member of the set — 133 PUTs were 403ing without it.
  # Named here on purpose: bumping this number is a decision, not a rubber stamp.
  # #4336: counted by a query over the file, not a text grep that a comment
  # mentioning "a chorus:Permission" would also match.
  printf '%s\n' 'PREFIX chorus: <https://jeffbridwell.com/chorus#>' \
    'SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { ?p a chorus:Permission }' > "$T/n.rq"
  n=$(arq --results csv --query "$T/n.rq" --data "$PERMS" 2>/dev/null | tail -1 | tr -d '\r')
  # 49 since #4432 (2026-10-06): permission-wren-events, for Wren's events domain (#4438).
  test "$n" -eq 49
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

# #4336: this case was `grep -q … && { false; }; true`, which bash's set -e
# never fails on, so it passed whatever the query returned. It now runs the
# query over one principal with a Read row, then the same principal with a
# Write row: 0 grants, then 1 — the two states it exists to separate.
perm_fixture() {  # perm_fixture <mode-term>
  printf '%s\n' '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
    '@prefix acl: <http://www.w3.org/ns/auth/acl#> .' '@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .' \
    'chorus:principal-fx a chorus:Principal ; chorus:webId "https://example.test/fx#me" .' \
    'chorus:permission-fx a chorus:Permission ; chorus:agent chorus:principal-fx ;' \
    "    chorus:accessTo \"urn:chorus:domains:fixture\"^^xsd:anyURI ; chorus:mode $1 ." > "$T/perm.ttl"
}

@test "NEGATIVE PROOF — a Read-mode row is not a write grant; the same row as Write is" {
  perm_fixture 'acl:Read';  r=$(rows --data "$T/perm.ttl")
  perm_fixture 'acl:Write'; w=$(rows --data "$T/perm.ttl")
  [ "$r" -eq 0 ] || { echo "a Read row granted write ($r)"; false; }
  [ "$w" -eq 1 ] || { echo "a Write row did not grant ($w) — the Read result above proves nothing"; false; }
}

@test "the real Read rows (nudge-read) do not come back as write grants" {
  run arq --results csv --query "$T/q.rq" --data "$PERMS" --data "$PRINCIPALS"
  [ "$status" -eq 0 ]
  test -z "$(printf '%s\n' "$output" | grep -F 'nudge-read' || true)"
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

@test "return gate — a hasScope-only principal gets no grant, an accessTo Write row does" {
  # #4336: was a grep of the .rq for "hasScope", "chorus:accessTo" and "acl:Write".
  printf '%s\n' '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
    'chorus:principal-hs a chorus:Principal ; chorus:webId "https://example.test/hs#me" ;' \
    '    chorus:hasScope "urn:chorus:domains:fixture" .' > "$T/hs.ttl"
  n=$(rows --data "$T/hs.ttl")
  [ "$n" -eq 0 ]
  perm_fixture 'acl:Write'; w=$(rows --data "$T/perm.ttl")
  [ "$w" -eq 1 ]
}

@test "return gate — the security deploy set carries the rows file and not the literals file" {
  # #4229 - the security set is a manifest row now, not a bash array.
  MAN="$ROOT/platform/config/domain-set-manifest.txt"
  grep -q "permissions-4183.ttl" "$MAN"
  # NEGATIVE PROOF: the literals file must not have come back with it.
  test -z "$(grep -F "security-scopes-3689.ttl" "$MAN" || true)"
}

# #3102 — every principal the cards CLI writes as (DEPLOY_ROLE jeff on accept and
# /card, the three roles otherwise) needs a Write row on the cards graph, or its
# graph write-through 403s. Jeff's was missing and the land's accept left #3102's
# row at WIP.
cards_writers_missing() {  # cards_writers_missing <perms.ttl> → names with no Write row
  printf '%s\n' 'PREFIX chorus: <https://jeffbridwell.com/chorus#>' \
    'PREFIX acl: <http://www.w3.org/ns/auth/acl#>' \
    'SELECT ?a WHERE { ?p a chorus:Permission ; chorus:agent ?a ; chorus:mode acl:Write ; chorus:accessTo ?g FILTER(STR(?g) = "urn:chorus:domains:cards") }' > "$T/cw.rq"
  have=$(arq --results csv --query "$T/cw.rq" --data "$1" 2>/dev/null | tail -n +2 | tr -d '\r')
  for w in jeff wren silas kade; do
    grep -qx "https://jeffbridwell.com/chorus#principal-$w" <<<"$have" || echo "$w"
  done
}

@test "every principal the cards CLI writes as can write the cards graph" {
  missing=$(cards_writers_missing "$PERMS")
  [ -z "$missing" ] || { echo "no cards Write row for: $missing"; false; }
}

@test "NEGATIVE PROOF — drop jeff's cards row and the check names jeff" {
  awk '/^chorus:permission-jeff-cards /{skip=1} skip&&/ \.$/{skip=0;next} !skip' "$PERMS" > "$T/nojeff.ttl"
  missing=$(cards_writers_missing "$T/nojeff.ttl")
  [ "$missing" = "jeff" ] || { echo "expected jeff missing, got: '$missing'"; false; }
}
