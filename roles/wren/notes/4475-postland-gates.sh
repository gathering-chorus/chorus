#!/bin/bash
# #4475 AC4 — the live proof, after the land: PATCH the 13 security Gate rows with
# {"implementedIn": null}. implementedIn was retired by #4471 (Gate uses implementedBy);
# a PUT could not remove it without also dropping rdfs:subClassOf on these punned rows.
# Run as a principal with Write on urn:chorus:domains:security (default silas; the rows
# have no owner, so the Permission row is what opens them). Read back from the store:
# implementedIn goes to 0, and every gate keeps its rdf:type and rdfs:subClassOf.
#   usage: bash 4475-postland-gates.sh [principal]        (dry run: DRY=1)
set -euo pipefail
ROOT=/Users/jeffbridwell/CascadeProjects/chorus
WHO="${1:-silas}"
C='https://jeffbridwell.com/chorus#'
source "$ROOT/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
# fuseki-auth.sh sets FUSEKI_AUTH as an ARRAY (-u user:pass); expanding it as a string asks for a password
q() { curl -s -m 30 ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} -H 'Accept: text/csv' --data-urlencode "query=$1" http://localhost:3030/pods/query | tail -n +2; }
count() { q "SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:domains:security> { ?s <${C}implementedIn> ?o } }"; }
shape() { q "SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <urn:chorus:domains:security> { ?s a <${C}Gate> ; <http://www.w3.org/2000/01/rdf-schema#subClassOf> ?c } }"; }

echo "before: implementedIn triples $(count) · gates with type+subClassOf $(shape)"
GATES=$(q "SELECT DISTINCT ?s WHERE { GRAPH <urn:chorus:domains:security> { ?s <${C}implementedIn> ?o } }" | sed "s|${C}gate-||" | tr -d '\r')
[ -n "${DRY:-}" ] && { echo "dry run: would patch: $GATES"; exit 0; }
export CHORUS_IDENTITY_TOKEN; CHORUS_IDENTITY_TOKEN=$(bash "$ROOT/platform/scripts/chorus-identity-token" "$WHO")
AM=$(command -v athena-model || echo "$ROOT/platform/services/athena-model/target/release/athena-model")
for g in $GATES; do
  "$AM" patch --path "/v1/security/gates/$g" --json '{"implementedIn":null}' || echo "  ^ $g refused"
done
echo "after:  implementedIn triples $(count) · gates with type+subClassOf $(shape)"
