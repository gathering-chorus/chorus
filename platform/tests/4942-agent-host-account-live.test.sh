#!/usr/bin/env bash
# @test-type: integration — reads the live principals from the store (read-only SPARQL)
# @domain: identity
# @needs: machine — prod after the land
# @card: #4942 · owner: silas
#
# #4942 AC3 — in prod, the shipped AgentRunsAsItsOwnAccount rule over the live
# principals reports 0 violations, so a rule that would refuse prod's own rows shows
# red here. Vacuity guard: fewer than 5 agent principals read is RED, never a pass.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
command -v shacl >/dev/null || { echo "RED: Jena shacl not on PATH"; exit 1; }
awk '/^chorus:AgentRunsAsItsOwnAccount a sh:SPARQLConstraint/,/""" \./' "$ROOT/roles/silas/ontology/security-model-3618.ttl" > "$TMP/rule.ttl"
[ -s "$TMP/rule.ttl" ] || { echo "RED: AgentRunsAsItsOwnAccount is not in security-model-3618.ttl"; exit 1; }
{ printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
  printf 'chorus:LiveGate a sh:NodeShape ; sh:targetClass chorus:Principal ; sh:sparql chorus:AgentRunsAsItsOwnAccount .\n'
  cat "$TMP/rule.ttl"; } > "$TMP/shapes.ttl"
C='https://jeffbridwell.com/chorus#'
curl -sf --max-time 15 -H 'Accept: text/turtle' --data-urlencode "query=CONSTRUCT { ?p a <${C}Principal> ; <${C}principalKind> ?k ; <${C}hostAccount> ?h }
  WHERE { GRAPH ?g { ?p a <${C}Principal> ; <${C}principalKind> ?k . OPTIONAL { ?p <${C}hostAccount> ?h } } }" "$QUERY" > "$TMP/data.ttl" \
  || { echo "RED: the store at $QUERY did not answer"; exit 1; }
agents=$(grep -c '"agent"' "$TMP/data.ttl")
[ "$agents" -ge 5 ] || { echo "RED: only $agents agent principal(s) read from the store, want >= 5"; exit 1; }
shacl validate --shapes "$TMP/shapes.ttl" --data "$TMP/data.ttl" > "$TMP/report.ttl" 2>"$TMP/err" || true
[ -s "$TMP/err" ] && { echo "RED: shacl errored:"; head -3 "$TMP/err"; exit 1; }
n=$(grep -c 'sh:focusNode' "$TMP/report.ttl")
[ "$n" -eq 0 ] || { echo "RED: $n agent principal(s) break the rule:"; grep -E 'focusNode|sh:value' "$TMP/report.ttl"; exit 1; }
echo "GREEN: $agents agent principals read, 0 break AgentRunsAsItsOwnAccount"
