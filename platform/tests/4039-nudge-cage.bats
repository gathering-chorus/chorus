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
# 4111 — the stub MUST be able to give up. handle_request() with no timeout
# blocks forever when the call never arrives, and the wait below blocks with it:
# measured 2026-09-07, this one test held a pipeline round for 18 minutes and
# counting at load 5 — not slow, stopped. A control that can hang forever is
# worse than one that fails: a hang has no verdict and takes the other 349 units
# of the lane down with it.
srv = HTTPServer(('127.0.0.1', port), H)
srv.timeout = 2
# Serve until the POST lands or 10s pass. One handle_request() was not enough
# once the test probed the port for readiness: the probe's empty connection was
# the one request, served and gone, and the real call found nobody listening.
import os, time
deadline = time.time() + 10
while not os.path.exists(out) and time.time() < deadline:
    srv.handle_request()
PY
  local stub=$!
  # Wait for the stub to LISTEN, up to 8s, instead of a fixed 0.4s. By hand this
  # suite is 3/3; in the 2026-09-16 03:48 nightly (43 min, box under load) the
  # control went red: python had not bound the port 0.4s in, curl got refused,
  # and a load-dependent race reported as a product failure. A control that
  # depends on how fast the box is that night cannot separate its states.
  local i
  for i in $(seq 1 80); do
    (echo > "/dev/tcp/127.0.0.1/$port") 2>/dev/null && break
    sleep 0.1
  done
  run env CHORUS_MCP_NUDGE_URL="http://127.0.0.1:$port/nudge" bash "$OPS_NUDGE" silas "stub probe"
  wait "$stub" 2>/dev/null || true
  [ "$status" -eq 0 ]
  # An empty file means the stub timed out uncalled. Say so, instead of leaving a
  # bare grep failure for someone to reverse-engineer.
  [ -s "$BATS_TEST_TMPDIR/got.txt" ] || {
    echo "the stub on 127.0.0.1:$port was never called — ops-nudge did not reach the seam"
    false
  }
  grep -q "stub probe" "$BATS_TEST_TMPDIR/got.txt"
}
