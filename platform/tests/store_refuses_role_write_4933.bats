#!/usr/bin/env bats
# @test-type: integration — one no-op update against the store as the account running the test (a role account in the land's prove), plus a local stub store for the negative proof.
# @domain: security
#
# #4933 AC5 — the door is the only writer: a role account holds no store write
# credential, so a direct update to the store is refused (rule 1). The store
# already requires localhost + basic auth on /update (shiro.ini:21-23, Silas
# 16:48); this proves a role's own environment carries nothing that passes it.
# The update is `INSERT DATA {}`: even a wrong pass writes nothing.
#
# NEGATIVE PROOF (#3734): against a stub store that ACCEPTS the update, the same
# check goes red — so a green here means the real store refused.

setup() {
  STORE="${CHORUS_STORE_UPDATE_URL:-http://localhost:3030/pods/update}"
  ME="$(id -un)"
}

# the refusal check: anything but 401/403 from the store is a failure. As a role
# account it runs here; as anyone else (the land's prove runs as the runner) it
# hops to the card's role through #4921's pinned hop (sudo -n -u chorus-<role> -H
# bash -c), no new rule. A refused hop is a SKIP, which borg-prove reads as
# unmeasured, never a pass (#4889, whole_run_4889.rs:35).
store_code() {
  local q="curl -s -o /dev/null -w '%{http_code}' --max-time 5 --data-urlencode 'update=INSERT DATA {}' '$1'"
  case "$ME" in
    chorus-*) bash -c "$q" ;;
    *) local role="${CHORUS_CARD_ROLE}"; [ -n "$role" ] || role=wren
       sudo -n -u "chorus-$role" -H bash -c "$q" 2>/dev/null || echo hop-refused ;;
  esac
}
store_refuses() {
  code=$(store_code "$1")
  echo "store answered $code to $ME"
  [ "$code" = 401 ] || [ "$code" = 403 ]
}

@test "a role account's direct update to the store is refused" {
  code=$(store_code "$STORE")
  echo "store answered $code to $ME (role chorus-${CHORUS_CARD_ROLE:-wren})"
  [ "$code" != hop-refused ] || skip "the role hop was refused for $ME: unmeasured, not a pass"
  [ "$code" = 401 ] || [ "$code" = 403 ]
}

@test "NEGATIVE PROOF: a store that accepts the update turns the check red" {
  port=$(( 40000 + RANDOM % 20000 ))
  python3 -c "
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length', 0))); self.send_response(200); self.end_headers()
    def log_message(self, *a): pass
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), H).serve_forever()" "$port" &
  pid=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s -o /dev/null "http://127.0.0.1:$port/" -X POST && break; sleep 0.2; done
  run store_refuses "http://127.0.0.1:$port/pods/update"
  kill "$pid"
  echo "$output"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: as a non-role account whose hop is refused, the check reads unmeasured, never green" {
  ME=runner-fixture CHORUS_CARD_ROLE=fx-no-such-role run store_code "$STORE"
  echo "$output"
  [ "$output" = hop-refused ]
  # and the case built on it skips (borg-prove: unmeasured), it never passes
  ! { [ "$output" = 401 ] || [ "$output" = 403 ]; }
}
