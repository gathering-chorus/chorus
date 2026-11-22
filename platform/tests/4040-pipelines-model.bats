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
  [[ "$output" == *"5"* ]] || return 1
  run sq "$PIPES" 'ASK { c:pipeline-cicd c:pipelineStatus "operating" }'
  [[ "$output" == *"yes"* || "$output" == *"true"* ]]
}

@test "AC3 athena pipeline has shape→forge→validate (#4432: the seed step is gone)" {
  run sq "$PIPES" 'SELECT (COUNT(?s) AS ?n) WHERE { c:pipeline-athena c:hasStep ?s }'
  [[ "$output" == *"3"* ]] || return 1
  run sq "$PIPES" 'ASK { c:pipeline-athena c:hasStep c:pipeline-step-athena-seed }'
  [[ "$output" == *"no"* || "$output" == *"false"* ]]
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
  # #4040 reopened — the listing is paged (100 of 282 on 10-03), so a grep of
  # page 1 went red the day the row moved past it. Ask for the row by name.
  run curl -sf --max-time 10 "$OWL_URL/documents/pipelines-design"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *'"id": "chorus:pipelines-design"'* ]] || return 1
}

# ── AC5: the daily runner emits a PipelineRun with metrics ──
@test "AC5 nightly runner emits a PipelineRun row with metrics + forPipeline" {
  NS="$REPO/platform/services/werk-test/src/run_record.rs"   # #4145 — the runner owns the emit
  grep -q 'pub fn pipeline_run_record_body' "$NS"   # #4541 pass 1: the thin pipeline_run_body wrapper was dead (no caller); all_scope.rs:853 emits through this one
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
  [[ "$output" == 4* ]] || return 1
}

# ── AC7 negative proofs ──
# #4544: was Witness, which #4576 homed (claimed by the werk domain), so the
# check went red for the right reason. Handoff is declared and claimed by no
# domain; the first assert fails loud if a later card claims it, so the
# subject is replaced instead of the check passing or failing for nothing.
@test "AC7 negative: an unclaimed class stays unmounted (Handoff has no route)" {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run; blocking-leg runs predate the model deploy by construction"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
  dir="$(mktemp -d "${BATS_TMPDIR:-/tmp}/plall.XXXXXX")"
  riot --output=nt "$REPO"/roles/*/ontology/*.ttl > "$dir/all.nt" 2>/dev/null
  run sq "$dir/all.nt" 'ASK { ?d c:definesVocabulary c:Handoff }'
  rm -rf "$dir"
  [[ "$output" == [Nn]o || "$output" == "false" ]] || { echo "Handoff is now claimed: pick another unclaimed class"; return 1; }
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$OWL_URL/handoffs"
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

# #4925 — the AC4047 live POST case was deleted: it refused prod (rightly) and no lane ran a
# variant, so it skipped every night. The positive write path is measured nightly by
# platform/tests/4145-runner-owns-the-run.bats: the run writes its own PipelineRun and the
# test reads it back carrying the dag run's id.

@test "AC4047 nightly emit authenticates and reports the real refusal" {
  NS="$REPO/platform/services/werk-test/src/all_scope.rs"   # #4145
  grep -q 'chorus-identity-token' "$NS"
  grep -q 'Authorization: Bearer' "$NS"
  # The mislabel that hid a 401 for a whole night must be gone from the CODE.
  # Grepping the whole file is the #3734 trap in reverse: the comment explaining
  # the fix contains the string, so a naive grep fails on a correct file. Strip
  # comments first, then assert no emitted message still says it.
  ! sed 's/#.*//' "$NS" | grep -q 'owl-api unreachable' || return 1
}
