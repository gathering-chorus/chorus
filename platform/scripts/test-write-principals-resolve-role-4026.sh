#!/usr/bin/env bash
# @test-type: security — daytime permission-row integrity
# test-write-principals-resolve-role-4026.sh — was #4026's "every write-scoped
# principal must also hold a role". #4183 retired that coupling: a permission is
# a row (acl:Authorization), a role is a hat, and the door no longer asks a
# permission-holder for a hat (Jeff, 2026-09-16 11:35: "isnt the deleting
# users an authz not a user or role"). The file keeps its name so the daytime
# health check keeps firing it; what it checks is now:
#
#   1. every Permission row's acl:agent is a chorus:Principal with a webId
#      (a grant to nobody is a grant nobody can use — and one nobody can revoke)
#   2. every Write row names a urn:chorus:* graph
#   3. MIGRATION: every hasScope literal still live has a Permission-row twin,
#      until the literals are retired (then this check says so by count)
#
# Reads only. Runs against the live store because the rows are the point.
set -uo pipefail
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
SECURITY_GRAPH="${CHORUS_ALLOW_SET_GRAPH:-urn:chorus:domains:security}"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  PASS: $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL: $1"; }
trap 'echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
q() {
  curl -s --max-time 10 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$FUSEKI_QUERY" \
    -H 'Accept: text/csv' --data-urlencode "query=$1" 2>/dev/null | tail -n +2 | tr -d '"\r'
}
P='PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX acl: <http://www.w3.org/ns/auth/acl#> '

echo "=== #4183 permission rows — $SECURITY_GRAPH ==="
total=$(q "$P SELECT (COUNT(?r) AS ?n) WHERE { GRAPH <$SECURITY_GRAPH> { ?r a c:Permission } }")
[ -n "$total" ] || { bad "store unreachable — UNMEASURED, not green"; exit 1; }
if [ "$total" -eq 0 ]; then
  bad "0 Permission rows in the store — the door reads rows now; zero rows means nobody may write anything (or the deploy did not land them)"
  exit 1
fi
ok "$total Permission rows"

# 1. every agent is a real principal
dangling=$(q "$P SELECT ?r WHERE { GRAPH <$SECURITY_GRAPH> { ?r a c:Permission ; c:agent ?p . FILTER NOT EXISTS { ?p a c:Principal ; c:webId ?w } } }")
if [ -z "$dangling" ]; then ok "every row's agent is a Principal with a webId"; else
  bad "rows granted to nobody (agent is not a Principal with a webId):"; echo "$dangling" | sed 's/^/        /'; fi

# 2. write rows name a urn:chorus:* graph
unknown=$(q "$P SELECT ?r ?g WHERE { GRAPH <$SECURITY_GRAPH> { ?r a c:Permission ; c:mode acl:Write ; c:accessTo ?g . FILTER(!STRSTARTS(STR(?g), \"urn:chorus:\")) } }")
if [ -z "$unknown" ]; then ok "every Write row names a urn:chorus:* graph"; else
  bad "Write rows naming a graph outside urn:chorus:*:"; echo "$unknown" | sed 's/^/        /'; fi

# 3. migration twin check
literals=$(q "$P SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$SECURITY_GRAPH> { ?p c:hasScope ?s } }")
if [ "${literals:-0}" -eq 0 ]; then
  ok "hasScope literals: 0 (retired) — nothing left to twin, which is the target state"
else
  orphan=$(q "$P SELECT ?p ?s WHERE { GRAPH <$SECURITY_GRAPH> { ?p c:hasScope ?s . FILTER NOT EXISTS { ?r a c:Permission ; c:agent ?p ; c:accessTo ?s2 . FILTER(STR(?s2) = STR(?s)) } } }")
  if [ -z "$orphan" ]; then ok "$literals hasScope literals still live, every one has a Permission-row twin (retire them with the #4183 DBA step)"; else
    bad "hasScope literals with NO Permission-row twin — a grant the door no longer honours:"; echo "$orphan" | sed 's/^/        /'; fi
fi
[ "$FAIL" -eq 0 ]
