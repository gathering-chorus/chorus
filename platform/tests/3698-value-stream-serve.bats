#!/usr/bin/env bats
# @test-type: integration — hermetic TTL guards (unit-shaped) PLUS live owl-api serve
# checks (service-hitting); classified integration so it skips-if-absent (#3528).
load test_helper
#
# #3698 — /valuestreams + /valuestreamsteps serve real rows. What Jeff sees: GET
# /valuestreams returns the streams with their steps (not an empty route), and the
# v2 page athena/value-stream.html renders. Root cause fixed here: ValueStreamShape
# + StepShape lacked chorus:instancesGraph, so owl-api read the default
# urn:chorus:ontology (which holds only the 3 DEPRECATED streams, #3697) instead of
# urn:chorus:instances (the 7 well-formed streams + 48 steps). Pure-ABox → instances
# (ADR-025), NOT the ontology graph the punned Domain/Service use.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CT="$REPO/roles/silas/ontology/chorus.ttl"
  DEPLOY="$REPO/platform/scripts/chorus-model-deploy.sh"
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
}

# ── AC2 (hermetic): value-stream-instances.ttl is governed-deployed, wipe-safe ──
@test "AC2 chorus-model-deploy hydrates value-stream-instances into urn:chorus:instances" {
  grep -q 'INSTANCE_SET=' "$DEPLOY"
  grep -q 'INSTANCE_GRAPH.*urn:chorus:instances' "$DEPLOY"
  grep -q 'value-stream-instances.ttl' "$DEPLOY"
}
@test "AC2 the instances merge is additive-only — NO retire clause (co-tenant wipe impossible)" {
  # the destructive #3593 retire leg (a FILTER-gated DELETE) must NEVER appear on the
  # INSTANCE_MERGE — that leg stays quarantined to the ontology graph.
  grep -q 'INSTANCE_MERGE=' "$DEPLOY"
  ! grep -q 'INSTANCE_MERGE=.*FILTER' "$DEPLOY"
}

# arq (portable across BSD/GNU, unlike grep -P) parses the committed TTL for the
# shape's instancesGraph — fail loud if arq is absent rather than false-green.
sq() {
  local qf; qf="$(mktemp "${BATS_TMPDIR:-/tmp}/vsq.XXXXXX.rq")"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\n%s\n' "$1" > "$qf"
  arq --data="$CT" --query="$qf" 2>/dev/null; rm -f "$qf"
}

@test "arq SPARQL engine is present (no false-green from a missing binary)" {
  command -v arq
}

# ── AC1 (hermetic): both value-stream shapes declare instancesGraph=instances ──
@test "AC1 ValueStreamShape declares instancesGraph urn:chorus:instances" {
  run sq 'ASK { c:ValueStreamShape c:instancesGraph "urn:chorus:instances" }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 StepShape declares instancesGraph urn:chorus:instances" {
  run sq 'ASK { c:StepShape c:instancesGraph "urn:chorus:instances" }'
  echo "$output" | grep -qi 'yes'
}
# Guard the category error the card AC made: value-stream is PURE ABox, so these
# shapes must NOT point at the ontology graph (that is the punned Domain/Service case).
@test "AC1 value-stream shapes do NOT point at the ontology graph" {
  run sq 'ASK { { c:ValueStreamShape c:instancesGraph "urn:chorus:ontology" } UNION { c:StepShape c:instancesGraph "urn:chorus:ontology" } }'
  echo "$output" | grep -qi 'no'
}

# ── AC3 (data precondition): the instances graph holds >=3 SHAPE-VALID streams ──
# (label+trigger+outcome+>=1 step) — proves the serve will return rows, not re-hit 0.
@test "AC3 urn:chorus:instances holds >=3 shape-valid ValueStreams" {
  command -v curl
  local q='PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT (COUNT(DISTINCT ?vs) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?vs a c:ValueStream ; rdfs:label ?l ; c:trigger ?t ; c:outcome ?o . ?s c:inStream ?vs } }'
  run curl -s --max-time 8 "$FUSEKI_QUERY" --data-urlencode "query=$q" -H 'Accept: text/csv'
  [ "$status" -eq 0 ]
  local n; n=$(echo "$output" | tail -1 | tr -dc '0-9')
  [ "${n:-0}" -ge 3 ]
}

# ── AC3/AC4 (live serve): the owl-api endpoints return the rows, no hard-refuse ──
# Hits OWL_URL (default :3360 canonical; the werk pipeline points it at the variant).
# skip-if-absent keeps it green where owl-api isn't running (unit-only envs).
@test "AC3 GET /valuestreams serves >=3 streams (no hard-refuse)" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$OWL_URL/valuestreams"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$OWL_URL/valuestreams"
  local count; count=$(echo "$output" | python3 -c "import sys,json; print(json.load(sys.stdin).get('count',0))" 2>/dev/null || echo 0)
  [ "${count:-0}" -ge 3 ]
}
@test "AC3 GET /valuestreamsteps serves steps" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$OWL_URL/valuestreamsteps"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$OWL_URL/valuestreamsteps"
  local count; count=$(echo "$output" | python3 -c "import sys,json; print(json.load(sys.stdin).get('count',0))" 2>/dev/null || echo 0)
  [ "${count:-0}" -ge 1 ]
}
