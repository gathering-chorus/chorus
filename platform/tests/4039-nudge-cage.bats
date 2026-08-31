#!/usr/bin/env bats
# @test-type: unit — hermetic: drives the REAL ops-nudge against (1) the caged
# dead-port URL the suite world hands every test process and (2) a local stub.
# #4039 — ops-nudge was the last uncaged outbound: suites shelling it
# (alert-delivery-test.sh, chorus-health) paged Jeff from inside every daily
# run. Negative proof (#3734): the cage is shown to separate its two states —
# caged → the nudge reaches NOTHING; seam to a stub → it delivers.

OPS_NUDGE="$BATS_TEST_DIRNAME/../scripts/ops-nudge"

@test "the suite world cages CHORUS_MCP_NUDGE_URL to the dead port" {
  # the runner is the authority: ask the crate's own env list
  run grep -A1 'CHORUS_MCP_NUDGE_URL' "$BATS_TEST_DIRNAME/../services/werk-test/src/lib.rs"
  [[ "$output" == *"127.0.0.1:9/nudge"* ]]
}

@test "NEGATIVE PROOF: under the caged URL, ops-nudge reaches nothing (typed transport error)" {
  run env CHORUS_MCP_NUDGE_URL="http://127.0.0.1:9/nudge" bash "$OPS_NUDGE" silas "caged probe — must not deliver"
  [ "$status" -eq 2 ]
  [[ "$output" == *"transport error"* ]]
}

@test "control: the same call with the seam at a live stub DELIVERS — the cage separates its states" {
  command -v python3 >/dev/null || skip "python3 not installed"
  local port=$(( ( $$ % 20000 ) + 20000 ))
  python3 - "$port" "$BATS_TEST_TMPDIR/got.txt" <<'PY' &
import sys, json
from http.server import BaseHTTPRequestHandler, HTTPServer
port, out = int(sys.argv[1]), sys.argv[2]
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        open(out, 'wb').write(body)
        self.send_response(200); self.send_header('Content-Type','application/json'); self.end_headers()
        self.wfile.write(b'{"ok":true}')
    def log_message(self, *a): pass
HTTPServer(('127.0.0.1', port), H).handle_request()
PY
  local stub=$!
  sleep 0.4
  run env CHORUS_MCP_NUDGE_URL="http://127.0.0.1:$port/nudge" bash "$OPS_NUDGE" silas "stub probe"
  wait "$stub" 2>/dev/null || true
  [ "$status" -eq 0 ]
  grep -q "stub probe" "$BATS_TEST_TMPDIR/got.txt"
}
