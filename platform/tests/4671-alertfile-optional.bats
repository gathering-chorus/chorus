#!/usr/bin/env bats
# @test-type: unit — a stub SPARQL endpoint on a local port; no store touched
# @domain: alerts
# @card: 4671 · owner: wren
# What Jeff sees: a Grafana-sourced Alert row is accepted because the old
# required-alertFile blank node is gone, and nothing else in the shape store moved
# (scripts/alertfile-optional-4671.sh; ontology/alerts-4085.ttl names the property).

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO/platform/scripts/alertfile-optional-4671.sh"
TTL="$REPO/roles/silas/ontology/alerts-4085.ttl"

# stub: answers every COUNT with $N and records each update body
start_stub() {
  STUB_DIR="$(mktemp -d)"; export STUB_DIR
  python3 - "$1" "$STUB_DIR" <<'EOF' &
import http.server, json, sys, urllib.parse
n, d = sys.argv[1], sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode()
        if self.path.endswith("/update"):
            open(d + "/updates", "a").write(body + "\n"); self.send_response(204); self.end_headers(); return
        out = json.dumps({"results": {"bindings": [{"n": {"value": n}}]}}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers(); self.wfile.write(out)
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(d + "/port", "w").write(str(s.server_address[1])); s.serve_forever()
EOF
  STUB_PID=$!
  for _ in $(seq 1 50); do [ -s "$STUB_DIR/port" ] && break; sleep 0.1; done
  PORT="$(cat "$STUB_DIR/port")"
}
teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null; true; }

@test "no dataset named: refused, never a default store" {
  run "$SCRIPT"
  [ "$status" -eq 2 ]
}

@test "count only by default: one old node is reported and nothing is deleted" {
  start_stub 1
  run "$SCRIPT" "http://127.0.0.1:$PORT/ds"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"old alertFile blank nodes=1"* ]] || return 1
  [ ! -e "$STUB_DIR/updates" ]
}

@test "negative proof: a count other than 1 is refused and no update is sent, even with --apply" {
  start_stub 2
  run "$SCRIPT" "http://127.0.0.1:$PORT/ds" --apply
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"refused"* ]] || return 1
  [ ! -e "$STUB_DIR/updates" ]
}

@test "the ttl names the alertFile property as optional and keeps no required blank node for it" {
  grep -q 'sh:property chorus:AlertShape-alertFile' "$TTL"
  grep -q 'chorus:AlertShape-alertFile a sh:PropertyShape' "$TTL"
  run grep -E '\[ sh:path chorus:alertFile *; sh:minCount 1' "$TTL"
  [ "$status" -ne 0 ]
}
