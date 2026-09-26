#!/usr/bin/env bats
# @test-type: integration — hermetic TTL guards (unit-shaped) PLUS live owl-api serve
# @domain: pipelines — the product domain this suite guards (#4334)
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
  STEPS="$REPO/roles/kade/ontology/pipeline-step-instances.ttl"
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

# REWRITTEN 2026-09-19 (#4187). This required both shapes to carry SOME
# instancesGraph pin. #4187 removed those pins: the pipelines domain claims
# Pipeline, PipelineRun and PipelineStep, so the home is derived from the claim
# and a pin is redundant. Requiring a pin now fails on a correct model.
#
# What still matters is that the shapes EXIST as node shapes - that is what the
# rest of this file builds on - so that is what is checked, plus the absence of
# a catch-all pin. Note the two `[[ ]]` asserts this replaces were also on
# non-final lines, which bash 3.2 swallows: they could not have failed here.
@test "AC2 PipelineShape + PipelineRunShape exist, with no v1 catch-all pin" {
  [ -f "$SHAPES" ]
  run sq "$SHAPES" 'ASK { c:PipelineShape a <http://www.w3.org/ns/shacl#NodeShape> }'
  test -n "$(printf '%s' "$output" | grep -iE 'yes|true' || true)"
  run sq "$SHAPES" 'ASK { c:PipelineRunShape a <http://www.w3.org/ns/shacl#NodeShape> }'
  test -n "$(printf '%s' "$output" | grep -iE 'yes|true' || true)"
  run sq "$SHAPES" 'ASK { c:PipelineShape c:instancesGraph "urn:chorus:instances" }'
  test -z "$(printf '%s' "$output" | grep -iE 'yes|true' || true)"
}

@test "AC2 pipelines-4040.ttl is in the MODEL_SET (never live-only, #3654)" {
  grep -q 'pipelines-4040.ttl' "$REPO/platform/services/athena-deploy/src/lib.rs"
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
  run sq "$STEPS" 'SELECT (COUNT(?s) AS ?n) WHERE { ?s a c:PipelineStep . FILTER NOT EXISTS { ?s c:executor ?e } }'
  [[ "$output" == *'"0"'* || "$output" == *"| 0 "* ]]
  run sq "$STEPS" 'ASK { c:pipeline-step-cicd-demo c:executor "human" }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

# ── AC1/AC5 wiring: instances are governed-deployed (wipe-safe, #3895 lane) ──
@test "AC1 pipeline-instances.ttl is in the instance-seed manifest" {
  [ -f "$MANIFEST" ]
  grep -q '^pipeline:roles/kade/ontology/pipeline-instances.ttl' "$MANIFEST"
  grep -q '^pipeline-step:roles/kade/ontology/pipeline-step-instances.ttl' "$MANIFEST"
  # two kinds share no file: the seeder refuses a subject claimed by two kinds
  # in one batch (proven at the #4040 land 19:23)
  [ "$(grep -cE '^(pipeline|pipeline-step):' "$MANIFEST")" = "2" ]
  [ "$(grep -E '^(pipeline|pipeline-step):' "$MANIFEST" | cut -d: -f2 | sort -u | wc -l | tr -d ' ')" = "2" ]
}

# ── AC6 (live): the generated API serves both collections from the claims ──
@test "AC6 GET /pipelines serves cicd + athena (live owl-api)" {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  run curl -sf --max-time 10 "$OWL_URL/pipelines"
  [ "$status" -eq 0 ]
  [[ "$output" == *"cicd"* && "$output" == *"athena"* ]]
}

@test "AC6 GET /pipelineruns is mounted (live owl-api)" {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
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
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  [ "$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$OWL_URL/documents")" = "200" ] \
    || skip "claim not deployed yet"
  run curl -sf --max-time 10 "$OWL_URL/documents"
  [[ "$output" == *"pipelines"* ]]
}

# ── AC5: the daily runner emits a PipelineRun with metrics ──
@test "AC5 nightly runner emits a PipelineRun row with metrics + forPipeline" {
  NS="$REPO/platform/services/werk-test/src/nightly_run.rs"   # #4145 — the runner owns the emit
  grep -q 'pub fn pipeline_run_body' "$NS"
  grep -q 'forPipeline' "$NS"
  grep -q 'testsFailed' "$NS"
  grep -q 'runDurationMs' "$NS"
}

@test "AC5/AC7 negative (live): POST /pipelineruns without forPipeline refuses" {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
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
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
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

# ── #4047: the POSITIVE write path. #4040 proved only refusals, which is how a
# door that rejects EVERY run row shipped looking green. This test writes a real
# row and reads it back; it is the check that would have caught it.
@test "AC4047 (live) a valid PipelineRun POSTs and reads back with its metrics" {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  # #4175 — this test WRITES, so it must never point at canonical. It had been
  # POSTing probe rows into the live store: 90 of the 122 rows on :3360 on
  # 2026-09-14 were probe-4047-*, written by this line over weeks. A write test
  # whose default target is production is the membrane class (#3615), and the
  # default here was :3360. It refuses rather than picking a target for you.
  case "$OWL_URL" in
    *:3360*) skip "refuses to write to the canonical store — point OWL_URL at a variant" ;;
  esac
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  [ "$(curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$OWL_URL/pipelineruns")" = "200" ] \
    || skip "route not deployed yet"
  TOK="$("$REPO/platform/scripts/chorus-identity-token" kade 2>/dev/null)"
  [ -n "$TOK" ]
  NAME="probe-4047-$$"
  run curl -s --max-time 15 -o /dev/null -w '%{http_code}' -X POST "$OWL_URL/pipelineruns" \
    -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' \
    -d "{\"name\":\"$NAME\",\"forPipeline\":\"pipeline-cicd\",\"traceId\":\"probe-4047\",\"runOutcome\":\"green\",\"runDurationMs\":\"1000\",\"testsRun\":\"3\",\"testsFailed\":\"0\",\"testsStored\":\"3\"}"
  [ "$output" = "201" ]
  # Read the ROW back, not a page of the collection. Scanning the list made this
  # go red the moment the collection passed the default page size of 100 — the
  # new row sorts last and falls off, so the check reported the product broken
  # when the only thing that had changed was how many rows existed. Asking for
  # the row by name cannot drift with the row count.
  run curl -sf --max-time 10 "$OWL_URL/pipelineruns/$NAME"
  [ "$status" -eq 0 ]
  [[ "$output" == *"$NAME"* ]]
}

@test "AC4047 nightly emit authenticates and reports the real refusal" {
  NS="$REPO/platform/services/werk-test/src/nightly_all.rs"   # #4145
  grep -q 'chorus-identity-token' "$NS"
  grep -q 'Authorization: Bearer' "$NS"
  # The mislabel that hid a 401 for a whole night must be gone from the CODE.
  # Grepping the whole file is the #3734 trap in reverse: the comment explaining
  # the fix contains the string, so a naive grep fails on a correct file. Strip
  # comments first, then assert no emitted message still says it.
  ! sed 's/#.*//' "$NS" | grep -q 'owl-api unreachable' 
}
