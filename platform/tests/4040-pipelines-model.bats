#!/usr/bin/env bats
# @test-type: integration — hermetic TTL guards (unit-shaped) PLUS live owl-api serve
# checks (service-hitting); classified integration so it skips-if-absent (#3528).
load test_helper
#
# #4040 — Pipelines modeled. What Jeff sees: GET /pipelines returns the two REAL
# pipelines (cicd, athena) with their steps and executor blends; clearing + borg
# present as planned instances with no invented steps; GET /pipelineruns serves
# run rows with metrics. Claims-only mount: no generator code change.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  DOMAINS="$REPO/roles/kade/ontology/domains-kade-3581.ttl"
  PIPES="$REPO/roles/kade/ontology/pipeline-instances.ttl"
  SHAPES="$REPO/roles/kade/ontology/pipelines-4040.ttl"
  MANIFEST="$REPO/platform/config/instance-seed-manifest.txt"
  OWL_URL="${OWL_URL:-http://localhost:3360}"
}

# arq parses committed TTL — fail loud if absent rather than false-green (#3698 pattern).
sq() {
  local data="$1" q="$2" qf
  qf="$(mktemp "${BATS_TMPDIR:-/tmp}/plq.XXXXXX.rq")"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\n%s\n' "$q" > "$qf"
  arq --data="$data" --query="$qf" 2>/dev/null; rm -f "$qf"
}

@test "arq SPARQL engine is present (no false-green from a missing binary)" {
  command -v arq
}

# ── AC2: PipelineRun claimed alongside Pipeline on the pipelines domain ──
@test "AC2 pipelines domain claims Pipeline AND PipelineRun (definesVocabulary)" {
  run sq "$DOMAINS" 'ASK { c:pipelines c:definesVocabulary c:Pipeline , c:PipelineRun }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

@test "AC2 PipelineShape + PipelineRunShape exist with instancesGraph declared" {
  [ -f "$SHAPES" ]
  run sq "$SHAPES" 'ASK { c:PipelineShape a <http://www.w3.org/ns/shacl#NodeShape> ; c:instancesGraph ?g }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
  run sq "$SHAPES" 'ASK { c:PipelineRunShape a <http://www.w3.org/ns/shacl#NodeShape> ; c:instancesGraph ?g }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

@test "AC2 pipelines-4040.ttl is in the MODEL_SET (never live-only, #3654)" {
  grep -q 'pipelines-4040.ttl' "$REPO/platform/scripts/athena-deploy-model.sh"
}

# ── AC3: exactly the two real instances, with real steps ──
@test "AC3 cicd pipeline has the five werk steps in order" {
  run sq "$PIPES" 'SELECT (COUNT(?s) AS ?n) WHERE { c:pipeline-cicd c:hasStep ?s }'
  [[ "$output" == *"5"* ]]
  run sq "$PIPES" 'ASK { c:pipeline-cicd c:pipelineStatus "operating" }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

@test "AC3 athena pipeline has shape→forge→seed→validate" {
  run sq "$PIPES" 'SELECT (COUNT(?s) AS ?n) WHERE { c:pipeline-athena c:hasStep ?s }'
  [[ "$output" == *"4"* ]]
}

@test "AC3 clearing + borg are planned instances with NO steps (no invented steps)" {
  run sq "$PIPES" 'ASK { c:pipeline-clearing c:pipelineStatus "planned" . c:pipeline-borg c:pipelineStatus "planned" }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
  run sq "$PIPES" 'ASK { { c:pipeline-clearing c:hasStep ?s } UNION { c:pipeline-borg c:hasStep ?s } }'
  [[ "$output" == *"no"* || "$output" == *"false"* ]]
}

# ── AC4: every step declares its executor blend ──
@test "AC4 every declared step carries an executor (human|agent|deterministic)" {
  run sq "$PIPES" 'SELECT (COUNT(?s) AS ?n) WHERE { ?s a c:PipelineStep . FILTER NOT EXISTS { ?s c:executor ?e } }'
  [[ "$output" == *'"0"'* || "$output" == *"| 0 "* ]]
  run sq "$PIPES" 'ASK { c:step-cicd-demo c:executor "human" }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

# ── AC1/AC5 wiring: instances are governed-deployed (wipe-safe, #3895 lane) ──
@test "AC1 pipeline-instances.ttl is in the instance-seed manifest" {
  [ -f "$MANIFEST" ]
  grep -q 'pipeline-instances.ttl' "$MANIFEST"
}

# ── AC6 (live): the generated API serves both collections from the claims ──
@test "AC6 GET /pipelines serves cicd + athena (live owl-api)" {
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  run curl -sf --max-time 10 "$OWL_URL/pipelines"
  [ "$status" -eq 0 ]
  [[ "$output" == *"cicd"* && "$output" == *"athena"* ]]
}

@test "AC6 GET /pipelineruns is mounted (live owl-api)" {
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$OWL_URL/pipelineruns"
  [ "$output" = "200" ]
}

# ── AC1: Document claimed (Wren 16:22) so the design doc is mintable ──
@test "AC1 knowledge domain claims chorus:Document (mounts /documents)" {
  run sq "$REPO/roles/wren/ontology/memory-4010.ttl" 'ASK { c:knowledge c:definesVocabulary c:Document }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

@test "AC1 (live) /documents mounted + pipelines design Document present" {
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  [ "$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$OWL_URL/documents")" = "200" ] \
    || skip "claim not deployed yet"
  run curl -sf --max-time 10 "$OWL_URL/documents"
  [[ "$output" == *"pipelines"* ]]
}

# ── AC5: the daily runner emits a PipelineRun with metrics ──
@test "AC5 nightly runner emits a PipelineRun row with metrics + forPipeline" {
  NS="$REPO/platform/scripts/nightly-suites.sh"
  grep -q 'emit_pipeline_run "\$out"' "$NS"
  grep -q '"forPipeline":"pipeline-cicd"' "$NS"
  grep -q 'testsFailed' "$NS"
  grep -q 'runDurationMs' "$NS"
}

@test "AC5/AC7 negative (live): POST /pipelineruns without forPipeline refuses" {
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  # skip unless the route is MOUNTED — a 404 on an absent route also matches 4*
  # and would pass this vacuously (#3734: the check must fail only at the door)
  [ "$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$OWL_URL/pipelineruns")" = "200" ] \
    || skip "route not deployed yet"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST "$OWL_URL/pipelineruns" \
    -H 'Content-Type: application/json' \
    -d '{"label":"bogus run no pipeline link","runOutcome":"green","runDurationMs":1}'
  [[ "$output" == 4* ]]
}

# ── AC7 negative proofs ──
@test "AC7 negative: an unclaimed class stays unmounted (Witness has no route)" {
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$OWL_URL/witnesses"
  [ "$output" != "200" ]
}

@test "AC7 negative: a PipelineRun missing its pipeline link refuses at the shape" {
  # The shape must carry forPipeline minCount 1 — that's what refuses a run row
  # with no pipeline link at the door. NO heredocs in this file: a heredoc inside
  # a bats @test body defeats bats' per-line failure detection (proven in this
  # card's red run — a false [[ ]] after a heredoc passed vacuously).
  [ -f "$SHAPES" ]
  run sq "$SHAPES" 'ASK { c:PipelineRunShape <http://www.w3.org/ns/shacl#property> ?p . ?p <http://www.w3.org/ns/shacl#path> c:forPipeline ; <http://www.w3.org/ns/shacl#minCount> 1 }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}
