#!/usr/bin/env bats
# @test-type: unit — the check reads a local stub standing in for the Clearing; the rest of chorus-health reads live, read-only
# @domain: services
# #4633 — clearing-external exists to catch a 127.0.0.1-only bind. The Clearing
# answers the LAN with a 302 to its public host by design (Kade, measured
# 10-18 19:52), which proves the bind; a redirect anywhere else does not.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
}

teardown() {
  [ -n "$STUB" ] && kill "$STUB" 2>/dev/null
  rm -rf "$T"
}

stub() { # stub <status> <location>
  python3 - "$1" "$2" > "$T/port" <<'PY' &
import http.server, sys, socketserver
code, loc = int(sys.argv[1]), sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(code)
        if loc != "-": self.send_header("Location", loc)
        self.end_headers()
    def log_message(self, *a): pass
s = socketserver.TCPServer(("127.0.0.1", 0), H)
print(s.server_address[1], flush=True)
s.serve_forever()
PY
  STUB=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$T/port" ] && break; sleep 0.2; done
}

line() {
  CHORUS_HEALTH_NO_EMIT=1 CHORUS_HOME_IP=127.0.0.1 CLEARING_PORT="$(cat "$T/port")" bash "$ROOT/platform/scripts/chorus-health" 2>&1 | grep "clearing-external"
}

@test "a 302 to the public Clearing host proves the LAN bind" {
  stub 302 "https://clearing.lightlifeurbangardens.com/health"
  out=$(line)
  [[ "$out" == *"✓ clearing-external"*"302 to the public host"* ]] || return 1
}

@test "negative proof: a 302 somewhere else is not a pass" {
  stub 302 "https://evil.example.com/health"
  out=$(line)
  [[ "$out" != *"✓ clearing-external"* ]] || return 1
}
