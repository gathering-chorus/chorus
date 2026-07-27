#!/usr/bin/env bats
# @test-type: integration — live owl-api (:3360) + chorus-api (:3340) value-stream surface
load test_helper
# 3707-value-stream-surface — contract pins for the value-stream chunk
# (#3697-#3703): the recursive top-level stream must serve correctly at the
# data and API layers, and the v1 surface must stay retired. These are the
# regressions that would otherwise land silently in Jeff's browser.
#
# Integration tier by declaration: these tests read the LIVE services and
# skip loudly when they're unreachable (never false-green on a down box).

OWL="http://localhost:3360"
API="http://localhost:3340"

owl_up() { curl -sf --max-time 5 "$OWL/" >/dev/null; }
api_up() { curl -sf --max-time 5 "$API/api/athena/health" >/dev/null; }

@test "valuestreams collection serves the chorus stream" {
  owl_up || skip "owl-api :3360 unreachable"
  run bash -c "curl -sf '$OWL/valuestreams' | python3 -c 'import json,sys; d=json.load(sys.stdin); names=[r[\"name\"] for r in d[\"data\"]]; assert \"value-stream-chorus\" in names, names; print(len(names))'"
  [ "$status" -eq 0 ]
}

@test "the six core steps carry stageOrder 1-6 exactly" {
  owl_up || skip "owl-api :3360 unreachable"
  run bash -c "curl -sf '$OWL/valuestreamsteps?limit=100' | python3 -c '
import json,sys
d=json.load(sys.stdin)
want={\"shaping\":1,\"directing\":2,\"designing\":3,\"building\":4,\"proving\":5,\"reflecting\":6}
got={r[\"name\"].replace(\"value-stream-step-\",\"\"):int(r[\"stageOrder\"]) for r in d[\"data\"] if r[\"name\"].replace(\"value-stream-step-\",\"\") in want}
assert got==want, got
print(\"ok\")'"
  [ "$status" -eq 0 ]
  [[ "$output" == *ok* ]]
}

@test "tree fold serves depth>=3 (stream -> step -> child stream -> its steps)" {
  owl_up || skip "owl-api :3360 unreachable"
  run bash -c "curl -sf '$OWL/valuestreams/value-stream-chorus/tree' | python3 -c '
import json,sys
t=json.load(sys.stdin)
def depth(n): return 1+max((depth(c) for c in n.get(\"children\",[])),default=0)
d=depth(t)
assert d>=4, f\"tree depth {d} < 4 (stream/step/child-stream/child-step)\"
print(d)'"
  [ "$status" -eq 0 ]
}

@test "v1 /api/athena/steps stays retired (410-class gone, points at owl-api)" {
  api_up || skip "chorus-api :3340 unreachable"
  run bash -c "curl -s '$API/api/athena/steps'"
  [[ "$output" == *"3702"* ]]
  [[ "$output" == *"valuestreams"* ]]
}

@test "athena-health catalog does not advertise a retired endpoint" {
  api_up || skip "chorus-api :3340 unreachable"
  run bash -c "curl -sf '$API/api/athena/health' | python3 -c '
import json,sys
d=json.load(sys.stdin)
paths=[q[\"path\"] for q in d[\"data\"][\"queries\"]]
assert \"/api/athena/steps\" not in paths, paths
print(\"ok\")'"
  [ "$status" -eq 0 ]
  [[ "$output" == *ok* ]]
}
