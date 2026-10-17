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
