#!/usr/bin/env bats
# @test-type: unit — the pair's store-write plan against a stub door; no live service
# @domain: pipelines — the rows pair.yaml is generated from reach the store (#4577)
# What Jeff sees: before any prod write, a dry run lists every row the pair
# pipeline needs (19 on an empty store), says SAME for rows already there, and
# writes nothing; an unreadable door gives no plan at all.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4577-rows"
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D"
  cat > "$BATS_TEST_TMPDIR/door.py" <<'EOF'
import http.server, json, os, sys
D = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        coll = self.path.split("?")[0].strip("/").replace("/", "_")
        p = os.path.join(D, coll + ".json")
        body = open(p).read() if os.path.exists(p) else '{"data": []}'
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
        self.wfile.write(body.encode())
    def do_POST(self):
        open(os.path.join(D, "writes"), "a").write(self.path + "\n")
        self.send_response(200); self.end_headers(); self.wfile.write(b"{}")
    do_PATCH = do_POST
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(D, "port"), "w").write(str(s.server_address[1]))
# ends itself: no test has to stop it
import time
s.timeout, end = 1, time.time() + 20
while time.time() < end:
    s.handle_request()
EOF
  python3 "$BATS_TEST_TMPDIR/door.py" "$D" >/dev/null 2>&1 3>&- &
  for _ in $(seq 1 50); do [ -s "$D/port" ] && break; perl -e 'select(undef,undef,undef,0.1)'; done
  export OWL_URL="http://127.0.0.1:$(cat "$D/port")"
}

@test "an empty store plans all 19 rows and writes nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 19 add, 0 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *"ADD    pipelines/pair "*'"onFailureRun": "${CHORUS_HOME}/platform/scripts/pair red'* ]] || return 1
  [[ "$output" == *"ADD    stepskills/pair-green-1 "*'"withArguments": "by=NAVIGATOR choices=green|stop"'* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "a row already in the store reads SAME" {
  echo '{"data":[{"name":"pair-end-1","label":"pair end 1: pair end: the For-Jeff summary, deregister","callsSkill":"skill-pair-end","forStep":"pipeline-step-pair-end","skillOrder":"1","withArguments":"","ownedBy":"principal-kade"}]}' > "$D/stepskills.json"
  run python3 "$PLAN"
  [[ "$output" == *"SAME   stepskills/pair-end-1"* ]] || return 1
  [[ "$output" == *"18 add, 0 change; 1 same"* ]] || return 1
}

@test "negative proof: a row that differs is a CHANGE, never SAME" {
  echo '{"data":[{"name":"pair-end-1","label":"x","callsSkill":"skill-pair-end","forStep":"pipeline-step-pair-end","skillOrder":"1","withArguments":"","ownedBy":"principal-kade"}]}' > "$D/stepskills.json"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE stepskills/pair-end-1"* ]] || return 1
  [[ "$output" != *"SAME   stepskills/pair-end-1"* ]] || return 1
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL=http://127.0.0.1:9 run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"ADD"* ]] || return 1
}

# #4577 reopen 1 — the door refused the pair row: 'hasStep', then
# 'onFailureRun', were not in PipelineShape. Every field the plan sends a
# Pipeline must be a path of the shape the door checks.
shape_has() {
  local qf="$BATS_TEST_TMPDIR/q.rq"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\nPREFIX sh: <http://www.w3.org/ns/shacl#>\nASK { c:PipelineShape sh:property/sh:path c:%s }\n' "$1" > "$qf"
  arq --data="$ROOT/roles/kade/ontology/pipelines-4040.ttl" --query="$qf" 2>/dev/null | grep -qiE 'yes|true'
}

@test "every field the plan sends a Pipeline is in PipelineShape" {
  command -v arq >/dev/null || { echo "arq not installed: unmeasured, not green"; false; }
  run python3 "$PLAN"
  keys=$(printf '%s\n' "$output" | sed -n 's/^ADD    pipelines\/pair  //p' | python3 -c 'import sys,json; print(" ".join(k for k in json.load(sys.stdin) if k not in ("name","label")))')
  [ -n "$keys" ] || return 1
  for k in $keys; do shape_has "$k" || { echo "PipelineShape lacks $k"; return 1; }; done
}

@test "negative proof: a field outside PipelineShape is caught" {
  command -v arq >/dev/null || { echo "arq not installed: unmeasured, not green"; false; }
  ! shape_has bogusField4577 || return 1
  shape_has onFailureRun || return 1
}

# #4577 reopen 1 — every write was refused and the line said "applied"
@test "negative proof: --apply with refused writes says REFUSED and exits 1, never applied" {
  printf '#!/bin/bash\necho "POST x → 403"; exit 1\n' > "$BATS_TEST_TMPDIR/runner"; chmod +x "$BATS_TEST_TMPDIR/runner"
  CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"— REFUSED 19 of 19"* ]] || return 1
  [[ "$output" != *"applied"* ]] || return 1
}

@test "--apply writes each row as the caller through the runner and says applied" {
  printf '#!/bin/bash\necho "$*" >> %s/calls; echo "POST x → 201"\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/runner"; chmod +x "$BATS_TEST_TMPDIR/runner"
  CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner" run python3 "$PLAN" --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"19 add, 0 change; 0 same — applied"* ]] || return 1
  [ "$(wc -l < "$BATS_TEST_TMPDIR/calls")" -eq 19 ]
  grep -q '^call athena-add domain=pipelines class=pipelines fields=.*"hasStep": "pair-start pair-green' "$BATS_TEST_TMPDIR/calls"
  grep -q '^call athena-add domain=skills class=stepskills fields=.*"callsSkill": "pair-start"' "$BATS_TEST_TMPDIR/calls"
}
