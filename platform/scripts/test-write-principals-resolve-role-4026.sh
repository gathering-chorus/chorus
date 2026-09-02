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
# Scope note: a scope is a DOOR scope only if it names a graph the store holds —
# that is the door's own rule (athena-make scope_allows: the token's scope must
# name the target graph). chorus-api surface scopes (urn:chorus:ops, nudge-read,
# index, cards, catalog, …) name no graph and never meet the door; a principal
# holding only those is INFO, not FAIL. #4060 (Silas, 2026-09-02): the first
# version kept a hand list of non-door scopes (just urn:chorus:ops) and went RED
# on principal-chorus-sdk holding nudge-read — the hand list, not the grant, was
# wrong. Now derived: door scopes = held scopes ∩ graphs in the store.
# DOOR_GRAPHS is the fixture seam (#3528); live, the store is asked.
#
# NEGATIVE PROOF (#3734): the assessor is exercised against a fixture assembled
# from captured store rows with the nightly edge REMOVED and must FAIL there,
# then against the same rows with the edge present and must PASS — before the
# live store is asked at all. A check that cannot go red on the violation it
# exists to catch does not gate.
set -uo pipefail

FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
SECURITY_GRAPH="${CHORUS_ALLOW_SET_GRAPH:-urn:chorus:domains:security}"
# Space-separated graph IRIs the door can write. Fixtures set it; live derives it.
DOOR_GRAPHS="${DOOR_GRAPHS:-}"
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

# Which of the HELD scopes name a graph the store holds — the door's own test.
# VALUES + FILTER EXISTS: one short-circuit probe per scope (49ms live) instead
# of a full-store DISTINCT ?g scan (8s+, times out). Args: scope IRIs.
ask_graphs() {
  local vals="" sc
  for sc in "$@"; do vals="$vals <$sc>"; done
  curl -s --max-time 8 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$FUSEKI_QUERY" \
    -H 'Accept: text/csv' --data-urlencode "query=SELECT ?g WHERE { VALUES ?g {$vals } FILTER EXISTS { GRAPH ?g { ?s ?p ?o } } }" 2>/dev/null \
    | tail -n +2 | tr -d '"\r' | tr '\n' ' '
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
    # shellcheck disable=SC2086
    door_scopes=$(printf '%s\n' "$scopes" | tr -d '"' | tr ' ' '\n' | grep -xF -f <(printf '%s\n' $DOOR_GRAPHS) | tr '\n' ' ')
    if [ -z "${door_scopes// /}" ]; then
      echo "  $label: INFO  $name — scope(s) [$scopes] name no graph the store holds; never meets the door"
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

# Fixture world: the graphs the door writes, as captured from the store.
DOOR_GRAPHS="urn:chorus:ontology urn:chorus:instances urn:chorus:domains:tests urn:chorus:domains:security"

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

# 2b. NEGATIVE PROOF for the hand-list defect (#4060, Silas's land 2026-09-02):
#     a principal holding ONLY chorus-api surface scopes and no role is INFO,
#     never RED — those scopes name no graph, so the door is never met.
FIX_SURFACE='p,scopes,role
https://jeffbridwell.com/chorus#principal-wren,urn:chorus:instances,https://jeffbridwell.com/chorus#role-wren
https://jeffbridwell.com/chorus#principal-chorus-sdk,urn:chorus:nudge-read urn:chorus:ops urn:chorus:index urn:chorus:cards urn:chorus:catalog,'
assess "$FIX_SURFACE" "fixture-surface" >/dev/null; r=$?
[ "$r" -eq 0 ] && ok "surface-only scopes (nudge-read, ops, index, cards, catalog) never meet the door — INFO, not RED" \
               || bad "surface-only principal read as door-writing (returned $r) — the check, not the grant, is wrong"
#     and the same principal holding a real graph scope with no role IS red —
#     the derivation must still separate the two states.
FIX_SURFACE_RED='p,scopes,role
https://jeffbridwell.com/chorus#principal-chorus-sdk,urn:chorus:nudge-read urn:chorus:instances,'
assess "$FIX_SURFACE_RED" "fixture-surface-red" >/dev/null; r=$?
[ "$r" -eq 1 ] && ok "a surface scope beside a graph scope with no role is still RED" \
               || bad "graph scope hidden behind a surface scope: assessor returned $r, not RED"

# 3. Silence is not green: a headerless / empty answer must not pass.
assess "" "fixture-silent" >/dev/null; r=$?
[ "$r" -eq 2 ] && ok "silence is not green: an unanswered query is refused, not passed" \
               || bad "silence-as-green: assessor returned $r on an empty store answer"

# 4. LIVE — the graph the door resolves from, right now; door graphs from the store.
LIVE=$(ask_store)
# shellcheck disable=SC2046
DOOR_GRAPHS=$(ask_graphs $(printf '%s\n' "$LIVE" | tail -n +2 | cut -d, -f2 | tr -d '"' | tr ' ' '\n' | grep . | sort -u))
if [ -z "${DOOR_GRAPHS// /}" ]; then
  bad "live: could not probe which held scopes name a store graph at $FUSEKI_QUERY — cannot tell door scopes from surface scopes; refusing to guess"
  exit 1
fi
echo "  live: graph <$SECURITY_GRAPH> via $FUSEKI_QUERY (door scopes held: ${DOOR_GRAPHS% })"
assess "$LIVE" "live"; lrc=$?
case $lrc in
  0) ok "live: every principal holding an athena-make write scope resolves a role" ;;
  1) bad "live: a write-scoped principal resolves no role — its writes 403 today and the 03:00 nightly will store nothing" ;;
  *) bad "live: could not ask <$SECURITY_GRAPH> at $FUSEKI_QUERY — refusing to read silence as green" ;;
esac

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
