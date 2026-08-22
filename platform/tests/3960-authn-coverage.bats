#!/usr/bin/env bats
# @test-type: integration — in-process python fixture server on a loopback port + the real script under test; hermetic (no live chorus surfaces)
# #3960 — negative proof for the authn-coverage gate (#3734).
# The gate must distinguish the three states it exists to separate:
#   guarded route (401)      -> COVERED
#   unguarded route (200)    -> OPEN        (the violation it must catch)
#   wrong/absent route (404) -> MISPROBE    (must NEVER read as covered)
# If any of these collapse, the gate cannot tell "auth works" from "I hit the
# wrong URL" — the exact failure this proof exists to prevent.

# Covers: platform/scripts/authn-coverage.sh  (literal path so the #3917
# coverage linker registers this suite against the script — it greps for a
# `platform/…​.sh` token, which a $BATS_TEST_DIRNAME-relative path hides).
SCRIPT="${BATS_TEST_DIRNAME}/../scripts/authn-coverage.sh"
FIXTURE_PORT=39601

setup_file() {
  # Tiny fixture: /guarded -> 401, /open -> 200. Anything else -> 404.
  python3 - "$FIXTURE_PORT" >/dev/null 2>&1 &
  echo $! > "${BATS_TMPDIR}/authn_fixture.pid"
  cat > "${BATS_TMPDIR}/authn_fixture.py" <<'PY'
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        code = {'/guarded': 401, '/open': 200}.get(self.path, 404)
        self.send_response(code); self.end_headers()
    def log_message(self, *a): pass
HTTPServer(('127.0.0.1', int(sys.argv[1])), H).serve_forever()
PY
  python3 "${BATS_TMPDIR}/authn_fixture.py" "$FIXTURE_PORT" >/dev/null 2>&1 &
  echo $! > "${BATS_TMPDIR}/authn_fixture.pid"
  # wait for listen
  for _ in $(seq 1 40); do
    curl -s -o /dev/null "http://127.0.0.1:${FIXTURE_PORT}/" && break
    sleep 0.1
  done
}

teardown_file() {
  [ -f "${BATS_TMPDIR}/authn_fixture.pid" ] && kill "$(cat "${BATS_TMPDIR}/authn_fixture.pid")" 2>/dev/null || true
}

@test "score(): only 401/403 are COVERED" {
  [ "$(bash "$SCRIPT" score 401)" = COVERED ]
  [ "$(bash "$SCRIPT" score 403)" = COVERED ]
  [ "$(bash "$SCRIPT" score 200)" = OPEN ]
  [ "$(bash "$SCRIPT" score 400)" = OPEN ]
  [ "$(bash "$SCRIPT" score 404)" = MISPROBE ]
  [ "$(bash "$SCRIPT" score 000)" = MISPROBE ]
}

@test "guarded fixture route scores COVERED" {
  run bash "$SCRIPT" probe "http://127.0.0.1:${FIXTURE_PORT}/guarded"
  [ "$status" -eq 0 ]
  [[ "$output" == COVERED* ]]
}

@test "NEGATIVE PROOF: an unguarded route scores OPEN, not COVERED" {
  run bash "$SCRIPT" probe "http://127.0.0.1:${FIXTURE_PORT}/open"
  [ "$status" -eq 0 ]
  [[ "$output" == OPEN* ]]
  [[ "$output" != COVERED* ]]
}

@test "NEGATIVE PROOF: a wrong/absent route scores MISPROBE, never COVERED" {
  run bash "$SCRIPT" probe "http://127.0.0.1:${FIXTURE_PORT}/does-not-exist"
  [ "$status" -eq 0 ]
  [[ "$output" == MISPROBE* ]]
  [[ "$output" != COVERED* ]]
}

# #3965 — bind scope decides whether an OPEN row is a real network hole or
# accepted loopback-trust. The risk() classifier must keep those two apart:
# a loopback OPEN is NOT a NET-GAP, or the tool over-counts trust as exposure.
@test "risk(): a COVERED surface is OK regardless of scope" {
  [ "$(bash "$SCRIPT" risk COVERED NETWORK)" = OK ]
  [ "$(bash "$SCRIPT" risk COVERED LOOPBACK)" = OK ]
}

@test "risk(): only a NETWORK-exposed OPEN is a NET-GAP" {
  [ "$(bash "$SCRIPT" risk OPEN NETWORK)" = NET-GAP ]
}

@test "NEGATIVE PROOF: a LOOPBACK OPEN is 'trust', NOT a NET-GAP" {
  run bash "$SCRIPT" risk OPEN LOOPBACK
  [ "$status" -eq 0 ]
  [ "$output" = trust ]
  [ "$output" != NET-GAP ]
}
