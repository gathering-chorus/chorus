#!/usr/bin/env bats
# @test-type: integration — NEGATIVE PROOFS (#3734) for 4279-api-quartet-prod.bats.
# Neither case writes production: one proves the label gate refuses, the other
# plants a probe row in a throwaway graph and proves the residue query finds it.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
RUNNER="$ROOT/platform/tests/4267-all-generated-apis.test.sh"
API="${QUARTET_API:-http://localhost:3360}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"

setup() {
  # shellcheck disable=SC1091
  [ -r "$ROOT/platform/scripts/fuseki-auth.sh" ] && source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null
  curl -sf --max-time 5 "$API/health" >/dev/null 2>&1 || skip "$API not answering"
}

@test "an unlabelled production run is refused (rc=3) and names the label" {
  run env API_BASE="$API" CHORUS_CONTEXT=test QUARTET_PROD= bash "$RUNNER"
  echo "$output" | head -3
  [ "$status" -eq 3 ]
  echo "$output" | grep -q "not labelled a production write"
}

@test "the residue query finds a planted leftover row and not an absent one" {
  G="urn:chorus:test:4279-fixture-$$"; RID="proof-$$"
  code="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "INSERT DATA { GRAPH <$G> { <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-planted> <https://jeffbridwell.com/chorus#label> \"planted\" } }" "$UPDATE")"
  case "$code" in 2*) ;; *) echo "could not plant the fixture row (HTTP $code)"; return 1 ;; esac
  left="$(curl -s --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -G "$QUERY" \
    --data-urlencode "query=SELECT DISTINCT ?g ?s WHERE { VALUES ?s { <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-planted> <https://jeffbridwell.com/chorus#test-result-zz-probe-$RID-absent> } GRAPH ?g { ?s ?p ?o } }" \
    -H 'Accept: text/csv' 2>/dev/null | tail -n +2 | tr -d '\r')"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -X POST -H 'Content-Type: application/sparql-update' --data-binary "DROP SILENT GRAPH <$G>" "$UPDATE"
  echo "found: $left"
  echo "$left" | grep -q "zz-probe-$RID-planted"
  ! echo "$left" | grep -q "zz-probe-$RID-absent"
}
