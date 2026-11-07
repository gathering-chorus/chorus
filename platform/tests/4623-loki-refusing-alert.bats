#!/usr/bin/env bats
# @test-type: contract — runs the loki-refusing rule's check (proving/domains/alerts/loki-refusing.yml)
# against a stub Prometheus on localhost; no real Loki, no real Prometheus.
# @domain: observability
# #4623 — "an alert when Loki refuses" (Wren/Jeff 2026-11-07 14:48). Negative proofs (#3734):
# refusals over the threshold fire, naming the count; Prometheus down is unmeasured, never ok.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  RULE="$REPO/proving/domains/alerts/loki-refusing.yml"
  CHECK="$BATS_TEST_TMPDIR/check.sh"
  awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{sub(/^  /, ""); print}' "$RULE" > "$CHECK"
  PORT=$(( 20000 + RANDOM % 20000 ))
  export PROM_URL="http://127.0.0.1:$PORT"
}

teardown() { [ -n "${STUB:-}" ] && kill "$STUB" 2>/dev/null || true; }

# a stub Prometheus answering every query with one value: refused for the 429|5xx query, total otherwise
stub() {
  python3 - "$PORT" "$1" "$2" >/dev/null 2>&1 <<'EOF' &
import sys, json, urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer
port, refused, total = int(sys.argv[1]), sys.argv[2], sys.argv[3]
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        q = urllib.parse.parse_qs(self.rfile.read(int(self.headers['Content-Length'])).decode()).get('query', [''])[0]
        v = refused if '429' in q else total
        body = json.dumps({"status": "success", "data": {"result": [{"value": [0, v]}]}}).encode()
        self.send_response(200); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
HTTPServer(('127.0.0.1', port), H).serve_forever()
EOF
  STUB=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s -o /dev/null "$PROM_URL" && break; sleep 0.2; done
}

@test "control: a few refusals under the threshold is ok, with the counts" {
  stub 3 1700
  run bash "$CHECK"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "ok refused=3 of 1700" || { echo "$output"; return 1; }
}

@test "negative proof: refusals at the threshold fire, naming the count" {
  stub 15 1691
  run bash "$CHECK"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "loki-refusing refused=15 of 1691" || { echo "$output"; return 1; }
}

@test "negative proof: Prometheus not answering is unmeasured and fires, never ok" {
  run bash "$CHECK"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "unmeasured" || { echo "$output"; return 1; }
}
