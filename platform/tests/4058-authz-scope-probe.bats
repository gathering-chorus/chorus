#!/usr/bin/env bats
# @test-type: unit
# 4058-authz-scope-probe.bats — proofs for authz-scope-probe.sh (#4058 AC2/AC3).
#
# The probe is a check that gates a demo verdict, so it ships with NEGATIVE
# PROOFS (#3734): a world where the unscoped caller is let in must make the
# probe FAIL, and a world where identity is not even evaluated (401) must not
# read as "refused correctly". A stub HTTP server plays the api; the probe is
# pointed at it via AUTHZ_API, so no live service is touched (#3528).

PROBE="$BATS_TEST_DIRNAME/../scripts/authz-scope-probe.sh"

# stub_api <scoped-code> <unscoped-code> — answers by bearer token.
stub_api() {
  local port; port=$(( 20000 + RANDOM % 20000 ))
  STUB_PORT=$port
  python3 - "$port" "$1" "$2" >/dev/null 2>&1 <<'PY' &
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
port, scoped, unscoped = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        auth = self.headers.get('Authorization', '')
        code = scoped if auth == 'Bearer scoped' else unscoped
        self.send_response(code); self.send_header('Content-Length', '0'); self.end_headers()
    def log_message(self, *a): pass
HTTPServer(('127.0.0.1', port), H).serve_forever()
PY
  STUB_PID=$!
  for _ in $(seq 1 40); do
    curl -s -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null && return 0
    sleep 0.1
  done
  return 1
}

teardown() { [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null || true; }

run_probe() {
  AUTHZ_API="http://127.0.0.1:$STUB_PORT" SCOPED_TOKEN=scoped UNSCOPED_TOKEN=unscoped \
    run bash "$PROBE"
}

@test "positive control: scoped 2xx + unscoped 403 -> pass" {
  stub_api 200 403 || { echo "stub did not come up" >&2; return 1; }
  run_probe
  [ "$status" -eq 0 ] || { echo "expected pass, got rc=$status: $output" >&2; return 1; }
  [[ "$output" == *"AUTHZ_SCOPE_PROBE=pass"* ]] || { echo "$output" >&2; return 1; }
}

@test "NEGATIVE PROOF (AC3): unscoped caller let in (200) -> probe FAILS" {
  stub_api 200 200 || { echo "stub did not come up" >&2; return 1; }
  run_probe
  [ "$status" -ne 0 ] || { echo "probe passed while the unscoped caller was let in: $output" >&2; return 1; }
  [[ "$output" == *"AC3 FAIL"* ]] || { echo "$output" >&2; return 1; }
}

@test "NEGATIVE PROOF (AC2): scoped caller still refused (403) -> probe FAILS" {
  stub_api 403 403 || { echo "stub did not come up" >&2; return 1; }
  run_probe
  [ "$status" -ne 0 ] || { echo "probe passed while the scoped caller was refused: $output" >&2; return 1; }
  [[ "$output" == *"AC2 FAIL"* ]] || { echo "$output" >&2; return 1; }
}

@test "401 on either leg is UNMEASURED, never a pass" {
  stub_api 401 401 || { echo "stub did not come up" >&2; return 1; }
  run_probe
  [ "$status" -ne 0 ] || { echo "probe passed on 401/401: $output" >&2; return 1; }
  [[ "$output" == *"UNMEASURED"* ]] || { echo "$output" >&2; return 1; }
}

@test "missing token env is a usage error (exit 2), not a verdict" {
  AUTHZ_API="http://127.0.0.1:1" run bash "$PROBE"
  [ "$status" -eq 2 ] || { echo "rc=$status: $output" >&2; return 1; }
}
