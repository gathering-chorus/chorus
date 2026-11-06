#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-4889-rows against a stand-in door; no prod
# @domain: pipelines
# @card: #4889 · owner: silas
# The store side of Jeff's 18:21 path: the dry run plans the demo rows from the fixtures
# and retires the old ones; --apply writes through the runner (here a stand-in that logs).
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
S="$ROOT/platform/scripts/land-4889-rows"

setup() {
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D/skills" "$D/properties"
  echo '{"data":[]}' > "$D/properties/properties"
  # the door as it stands: the old demo rows and werk-review, and no code/arch/quality stepskills
  echo '{"data":[{"name":"werk-review"},{"name":"werk-demo-gate-product"},{"name":"werk-demo-validate","label":"demo: validate (WIP, has AC)","executor":"deterministic","implementedBy":"werk-demo","arguments":"validate ${CARD} ${ROLE}","ownedBy":"principal-wren","runsAs":"caller"}]}' > "$D/skills/skills"
  echo '{"data":[{"name":"cicd-test-2"},{"name":"demo-smoke-session-smoke"}]}' > "$D/stepskills"
  echo '{"data":[{"name":"demo-smoke","stepOrder":"3"},{"name":"demo-announce","stepOrder":"4"}]}' > "$D/pipelinesteps"
  echo '{"data":[{"name":"demo","hasStep":"pipeline-step-demo-smoke"}]}' > "$D/pipelines"
  PORT=$(( 20000 + RANDOM % 20000 ))
  (cd "$D" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) &
  SRV=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s -o /dev/null "http://127.0.0.1:$PORT/pipelines" && break; sleep 0.2; done
  printf '#!/bin/sh\necho "$*" >> "%s/calls"\necho "call → 200"\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/runner"; chmod +x "$BATS_TEST_TMPDIR/runner"
}
teardown() { kill "$SRV" 2>/dev/null || true; }

@test "the dry run plans the three reviews, step 3 announce, and retires the old rows; it writes nothing" {
  run env OWL_URL="http://127.0.0.1:$PORT" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  for want in "ADD    stepskills/demo-gates-gate-code" "ADD    stepskills/demo-gates-gate-arch" "ADD    stepskills/demo-gates-gate-quality" \
              'CHANGE pipelinesteps/demo-announce  {"stepOrder": "3"}' "DELETE stepskills/cicd-test-2" "DELETE pipelinesteps/demo-smoke" \
              "DELETE skills/skills/werk-review" "DELETE skills/skills/werk-demo-gate-product" \
              "ADD    properties/properties/prop-tool-path-semgrep" "ADD    properties/properties/prop-tool-path-claude" "nothing written (dry run)"; do
    [[ "$output" == *"$want"* ]] || { echo "missing: $want"; echo "$output"; return 1; }
  done
  [ ! -f "$BATS_TEST_TMPDIR/calls" ] || { echo "a dry run wrote: $(cat "$BATS_TEST_TMPDIR/calls")"; return 1; }
}

@test "--apply writes each planned row through the runner, deletes by id" {
  run env OWL_URL="http://127.0.0.1:$PORT" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q 'call athena-delete domain=skills class=skills id=werk-review' "$BATS_TEST_TMPDIR/calls" || { cat "$BATS_TEST_TMPDIR/calls"; return 1; }
  grep -q 'call athena-add domain=skills class=stepskills' "$BATS_TEST_TMPDIR/calls" || return 1
}

@test "--only properties/properties plans just the property rows: two tool paths, pulse's cooldown and what Jeff uses" {
  run env OWL_URL="http://127.0.0.1:$PORT" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S" --only properties/properties
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"plan: door writes 4 add, 0 change, 0 delete; 0 same"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"prop-pulse-announce-machine-cooldown-ms"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *stepskills* && "$output" != *"skills/skills"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a door it cannot read is no plan (exit 2), never a guess" {
  run env OWL_URL="http://127.0.0.1:9" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" python3 "$S"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/calls" ] || return 1
}
