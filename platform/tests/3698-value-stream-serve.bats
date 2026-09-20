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
  # #3991: repointed — #3561 renamed chorus-model-deploy.sh → athena-deploy-model.sh;
  # grep -c against the dead path failed both AC2 checks vacuously.
  DEPLOY="$REPO/platform/services/athena-deploy/target/release/athena-deploy"
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
}

# ── AC2 (hermetic): value-stream-instances.ttl is governed-deployed, wipe-safe ──
@test "AC2 instances hydrate via athena-model seed --deploy from the committed manifest (#3895)" {
  # #3904 — re-pointed: the INSTANCE_SET lane was deliberately REMOVED from
  # chorus-model-deploy.sh by #3895 (recovery must never carry the DAL gate).
  MANIFEST="$REPO/platform/config/instance-seed-manifest.txt"
  [ -f "$MANIFEST" ]
  grep -q 'value-stream-instances.ttl' "$MANIFEST"
  # and the recovery script must NOT regrow the lane (mirrors 3785 guard):
  run grep -c 'INSTANCE_SET=' "$DEPLOY"
  [ "$output" = "0" ]
}
@test "AC2 instances writes carry no retire clause (co-tenant wipe impossible, #3895 lane)" {
  # #3904 re-point: the bash INSTANCE_MERGE lane is GONE (#3895). Additivity now
  # lives in the DAL (seed_multi: per-subject delete-then-insert of staged
  # subjects only — covered by chorus-model crate tests). What this file can
  # still hold: the lane must not REGROW here, and the destructive RETIRE_ABSENT
  # leg stays quarantined to the ontology graph, never the instances graph.
  run grep -c 'INSTANCE_MERGE=' "$DEPLOY"
  [ "$output" = "0" ]
  ! grep -E 'RETIRE_ABSENT.*INSTANCE_GRAPH|INSTANCE_GRAPH.*RETIRE_ABSENT' "$DEPLOY"
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
# REWRITTEN 2026-09-19 (#4187). These two asserted that the shapes PIN
# instancesGraph to the v1 catch-all. That was the correct world when #3698
# landed and it is the world this card removes: the value-streams domain claims
# both classes, so athena-make derives urn:chorus:domains:value-streams from the
# claim (resolve_instances_graph, lib.rs:899) and a pin to the catch-all would
# now send reads and writes to a graph the rows have left.
#
# The property worth guarding did not change - each shape has ONE home and it is
# the value-streams graph - so that is what is asserted, and the catch-all pin is
# asserted ABSENT. A test that says "declares the catch-all" can only ever hold
# the old world in place.
# The claim half is deliberately NOT asserted here: `sq` queries this file's own
# TTL, and the value-streams domain makes its definesVocabulary claim in a
# different file. Asserting it here would fail on a correct model - the same
# too-narrow-reader mistake made once already today in 4187-service-home.
@test "AC1 ValueStream's home is not the v1 catch-all" {
  run sq 'ASK { c:ValueStreamShape c:instancesGraph "urn:chorus:instances" }'
  test -z "$(printf '%s' "$output" | grep -iE 'yes|true' || true)"
}
@test "AC1 Step's home is not the v1 catch-all" {
  run sq 'ASK { c:StepShape c:instancesGraph "urn:chorus:instances" }'
  test -z "$(printf '%s' "$output" | grep -iE 'yes|true' || true)"
}
# Guard the category error the card AC made: value-stream is PURE ABox, so these
# shapes must NOT point at the ontology graph (that is the punned Domain/Service case).
@test "AC1 value-stream shapes do NOT point at the ontology graph" {
  run sq 'ASK { { c:ValueStreamShape c:instancesGraph "urn:chorus:ontology" } UNION { c:StepShape c:instancesGraph "urn:chorus:ontology" } }'
  echo "$output" | grep -qi 'no'
}

# ── AC3 (data precondition): the instances graph holds >=3 SHAPE-VALID streams ──
# (label+trigger+outcome+>=1 step) — proves the serve will return rows, not re-hit 0.
@test "AC3 the value-streams domain graph holds >=3 shape-valid ValueStreams" {
  # 2026-09-18: #4187 moved ValueStream out of the catch-all into its own domain graph
  # (Jeff: a row's home is its domain's graph). Measured after the move — 8 streams in
  # urn:chorus:domains:value-streams, 0 in urn:chorus:instances. The old assertion was
  # asserting the thing the card exists to end, so it went red for being right.
  # The graph is read from the shape, not hardcoded, so the next move cannot strand it.
  # Read from the shape was the obvious move and it does not work yet: as of
  # 2026-09-18 ValueStreamShape declares NO instancesGraph at all and StepShape still
  # declares urn:chorus:instances, while every row has moved. The model has not caught
  # up with the data — raised with Wren on #4187. Until it does, the domain graph is
  # named here, and this comment is the reason it is a literal.
  command -v curl
  local vg="urn:chorus:domains:value-streams"
  local q="PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT (COUNT(DISTINCT ?vs) AS ?n) WHERE { GRAPH <$vg> { ?vs a c:ValueStream ; rdfs:label ?l ; c:trigger ?t ; c:outcome ?o . ?s c:inStream ?vs } }"
  run curl -s --max-time 8 "$FUSEKI_QUERY" --data-urlencode "query=$q" -H 'Accept: text/csv'
  test "$status" -eq 0
  local n; n=$(echo "$output" | tail -1 | tr -dc '0-9')
  test "${n:-0}" -ge 3
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
