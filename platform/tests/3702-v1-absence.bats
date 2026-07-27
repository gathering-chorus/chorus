#!/usr/bin/env bats
# @test-type: integration — hermetic absence guards (grep/file) + one live 410 check
# (skip only when chorus-api is absent, #3528).
# retirement-gate: absence-guard — this test NAMES deleted surfaces to assert they
# stay gone; the #3598 gate skips marker-declared absence-guards (#3702).
load test_helper
#
# #3702 — the v1 value-stream surface is GONE and cannot creep back. The only path
# to value-stream data is the v2 owl-api (/valuestreams, #3697-3699, guarded red-on-
# empty by #3701). Retired here: /api/athena/steps + steps.sparql + step-detail.html/
# .js + the STEPS_RULED / STREAM_NAME hardcoded step lists (the v1 Vertebra spine).
# SCOPE BOUNDARY: tree.json is load-bearing far beyond value-streams and is NOT
# retired by this card — the last two tests pin it in place.

setup() {
  API="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/api"
  SDK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/chorus-sdk"
  CHORUS_API="${CHORUS_API:-http://localhost:3340}"
}

# ── the four v1 files are gone ──
@test "v1 handler athena-steps.ts is deleted" {
  [ ! -e "$API/src/handlers/athena-steps.ts" ]
}
@test "v1 query steps.sparql is deleted" {
  [ ! -e "$API/src/sparql/steps.sparql" ]
}
@test "v1 page step-detail.html + step-detail.js are deleted" {
  [ ! -e "$API/public/athena/step-detail.html" ]
  [ ! -e "$API/public/athena/step-detail.js" ]
}

# ── nothing references the v1 surface (creep-back guard) ──
@test "no source references to athena-steps / steps.sparql remain" {
  run grep -rn "athena-steps\|steps\.sparql" "$API/src" --include='*.ts'
  [ "$status" -ne 0 ]
}
@test "no page links to step-detail remain" {
  run grep -rn "step-detail" "$API/src" "$API/public" \
    --include='*.ts' --include='*.js' --include='*.html'
  [ "$status" -ne 0 ]
}
@test "STEPS_RULED hardcode is deleted from athena-flow.js" {
  run grep -n "STEPS_RULED" "$API/public/athena/athena-flow.js"
  [ "$status" -ne 0 ]
}
@test "STREAM_NAME hardcode is deleted from chorus-sdk emit.ts" {
  run grep -n "STREAM_NAME" "$SDK/src/emit.ts"
  [ "$status" -ne 0 ]
}

# ── live: the v1 endpoint refuses with 410 Gone, pointing at v2 ──
@test "GET /api/athena/steps returns 410 (gone, not serving)" {
  run curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$CHORUS_API/api/athena/health"
  [ "$status" -eq 0 ] || skip "chorus-api not reachable at $CHORUS_API"
  run curl -s --max-time 5 -o /dev/null -w '%{http_code}' "$CHORUS_API/api/athena/steps"
  [ "$output" = "410" ]
}

# ── scope boundary: tree.json untouched and load-bearing consumers intact ──
@test "tree.json exists and parses (NOT retired by this card)" {
  local tj
  tj=$(find "$(cd "$BATS_TEST_DIRNAME/../.." && pwd)" -maxdepth 3 -name 'tree.json' -not -path '*/node_modules/*' | head -1)
  [ -n "$tj" ]
  python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$tj"
}
@test "tree.json consumers still reference it (nothing severed)" {
  run grep -rln "tree\.json" "$API/src" --include='*.ts'
  [ "$status" -eq 0 ]
}
