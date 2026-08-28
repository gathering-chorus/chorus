#!/usr/bin/env bash
# @test-type: security — daytime write-principal role resolution
# test-write-principals-resolve-role-4026.sh — #4026: every Principal that holds a
# write scope on a graph the athena-make door serves must ALSO resolve a role
# through chorus:holdsRole, because the door refuses any write whose caller has
# no model-resolved role ("writes require a model-resolved role", lib.rs
# write-authz-role). Identity + scope without the edge is a principal that looks
# provisioned and can write nothing.
#
# WHY THIS RUNS IN DAYLIGHT: on 2026-08-28 the 03:00 nightly ran 36/40 green and
# stored 0/4,229 TestResults — principal-nightly had scope + identity and no
# holdsRole edge, and the only thing that ever checked was the door itself at
# 3 AM. This check asks the STORE (not the TTL) the same question the door asks,
# from the same graph the door resolves, so the class fails at noon.
#
# Asks the store, never the file: the door resolves from urn:chorus:domains:security
# (chorus-oidc allow_set_graph); a TTL that carries the edge but was never
# deployed is exactly the landed≠live shape this exists to catch.
#
# Scope note: urn:chorus:ops is chorus-api's requiresScope (security-3619-
# surfaces*.ttl), not an athena-make write graph — a principal holding ONLY that
# scope never meets the door and is reported as INFO, not FAIL.
#
# NEGATIVE PROOF (#3734): the assessor is exercised against a fixture assembled
# from captured store rows with the nightly edge REMOVED and must FAIL there,
# then against the same rows with the edge present and must PASS — before the
# live store is asked at all. A check that cannot go red on the violation it
# exists to catch does not gate.
set -uo pipefail

FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
SECURITY_GRAPH="${CHORUS_ALLOW_SET_GRAPH:-urn:chorus:domains:security}"
NON_DOOR_SCOPES="${NON_DOOR_SCOPES:-urn:chorus:ops}"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  PASS: $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL: $1"; }
trap 'echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true

# The door's own question, widened to every scoped principal: who holds a write
# scope, and does that same subject hold a role. CSV: p,scopes,role
QUERY='PREFIX c: <https://jeffbridwell.com/chorus#>
SELECT ?p (GROUP_CONCAT(STR(?sc);separator=" ") AS ?scopes) (SAMPLE(STR(?r)) AS ?role)
WHERE { GRAPH <'"$SECURITY_GRAPH"'> { ?p a c:Principal ; c:hasScope ?sc . OPTIONAL { ?p c:holdsRole ?r } } }
GROUP BY ?p'

ask_store() {
  curl -s --max-time 8 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$FUSEKI_QUERY" \
    -H 'Accept: text/csv' --data-urlencode "query=$QUERY" 2>/dev/null
}

# assess <csv> <label> — prints one line per scoped principal, returns 1 on any
# door-writing principal without a role, 2 when the store did not answer.
# SINGLE-REQUEST TRUTH: a CSV without the header or without rows is "could not
# ask", never a pass.
assess() {
  local csv="$1" label="$2" rc=0 rows=0
  if ! printf '%s\n' "$csv" | head -1 | grep -q '^p,scopes,role'; then
    echo "  $label: store did not answer this query (no header) — refusing to read silence as green"
    return 2
  fi
  while IFS=, read -r p scopes role; do
    [ -z "$p" ] && continue
    rows=$((rows+1))
    local name="${p##*#}"
    local door_scopes
    door_scopes=$(printf '%s\n' "$scopes" | tr -d '"' | tr ' ' '\n' | grep -vxF -f <(printf '%s\n' $NON_DOOR_SCOPES) | tr '\n' ' ')
    if [ -z "${door_scopes// /}" ]; then
      echo "  $label: INFO  $name — scope(s) [$scopes] are not athena-make write graphs; never meets the door"
    elif [ -z "$(printf '%s' "$role" | tr -d '"\r')" ]; then
      echo "  $label: RED   $name — holds write scope(s) [${door_scopes% }] and resolves NO role → the door 403s every write (\"writes require a model-resolved role\")"
      rc=1
    else
      echo "  $label: ok    $name → ${role##*#} (scopes: ${door_scopes% })"
    fi
  done < <(printf '%s\n' "$csv" | tail -n +2 | tr -d '\r')
  [ "$rows" -gt 0 ] || { echo "  $label: zero scoped principals returned — the door would refuse everyone; not a pass"; return 2; }
  return $rc
}

# 1. NEGATIVE PROOF — captured 2026-08-28 store rows, nightly edge absent (the
#    exact state that stored 0/4,229). The assessor MUST go red here.
FIX_RED='p,scopes,role
https://jeffbridwell.com/chorus#principal-wren,urn:chorus:instances urn:chorus:domains:tests,https://jeffbridwell.com/chorus#role-wren
https://jeffbridwell.com/chorus#principal-nightly,urn:chorus:domains:tests,
https://jeffbridwell.com/chorus#principal-chorus-sdk,urn:chorus:ops,'
assess "$FIX_RED" "fixture-red" >/dev/null; r=$?
[ "$r" -eq 1 ] && ok "negative proof: assessor goes RED when a write-scoped principal has no holdsRole edge" \
               || bad "negative proof: assessor returned $r (not RED) on a fixture where principal-nightly holds tests scope and no role"

# 2. The same rows with the edge authored — the assessor must be able to pass.
FIX_GREEN='p,scopes,role
https://jeffbridwell.com/chorus#principal-wren,urn:chorus:instances urn:chorus:domains:tests,https://jeffbridwell.com/chorus#role-wren
https://jeffbridwell.com/chorus#principal-nightly,urn:chorus:domains:tests,https://jeffbridwell.com/chorus#role-nightly
https://jeffbridwell.com/chorus#principal-chorus-sdk,urn:chorus:ops,'
assess "$FIX_GREEN" "fixture-green" >/dev/null; r=$?
[ "$r" -eq 0 ] && ok "positive proof: assessor passes when every door-writing principal resolves a role" \
               || bad "positive proof: assessor returned $r on a fixture where every door-writing principal has a role"

# 3. Silence is not green: a headerless / empty answer must not pass.
assess "" "fixture-silent" >/dev/null; r=$?
[ "$r" -eq 2 ] && ok "silence is not green: an unanswered query is refused, not passed" \
               || bad "silence-as-green: assessor returned $r on an empty store answer"

# 4. LIVE — the graph the door resolves from, right now.
LIVE=$(ask_store)
echo "  live: graph <$SECURITY_GRAPH> via $FUSEKI_QUERY"
assess "$LIVE" "live"; lrc=$?
case $lrc in
  0) ok "live: every principal holding an athena-make write scope resolves a role" ;;
  1) bad "live: a write-scoped principal resolves no role — its writes 403 today and the 03:00 nightly will store nothing" ;;
  *) bad "live: could not ask <$SECURITY_GRAPH> at $FUSEKI_QUERY — refusing to read silence as green" ;;
esac

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
