#!/usr/bin/env bats
# index-crawler-snapshots.bats — #3068 resilience.
# What the team sees: com.chorus.crawler-index must SURVIVE a transient chorus-api
# outage (e.g. mid-redeploy) — retry with backoff and exit cleanly — instead of
# SIGPIPE-crash-looping (exit 141) into throttle-off on every deploy.

SCRIPT="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/index-crawler-snapshots.sh"
[ -f "$SCRIPT" ] || SCRIPT="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/index-crawler-snapshots.sh"

setup() {
  TEST_HOME=$(mktemp -d)
  export TEST_DB="$TEST_HOME/index.db"
  # a valid db so the DB precheck passes — we're exercising the chorus-api path
  sqlite3 "$TEST_DB" "CREATE TABLE messages (id INTEGER PRIMARY KEY);" 2>/dev/null
  # stub chorus-log at the path the script resolves (CHORUS_ROOT/platform/scripts/chorus-log)
  export TEST_ROOT="$TEST_HOME/root"
  mkdir -p "$TEST_ROOT/platform/scripts"
  printf '#!/bin/bash\nexit 0\n' > "$TEST_ROOT/platform/scripts/chorus-log"
  chmod +x "$TEST_ROOT/platform/scripts/chorus-log"
  # discard port — chorus-api is unavailable here
  export DEAD_API="http://127.0.0.1:9"
}

teardown() { rm -rf "$TEST_HOME"; }

# AC1 + AC2: a chorus-api outage does NOT crash the agent. It exits cleanly (0) —
# never 141 (SIGPIPE) or non-zero — so KeepAlive does not throttle it off.
@test "chorus-api unavailable: retries then exits cleanly (0), not 141 or 1" {
  run env API_URL="$DEAD_API" DB_PATH="$TEST_DB" CHORUS_ROOT="$TEST_ROOT" \
    HEALTH_RETRY_MAX=2 HEALTH_RETRY_DELAY=0 bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

# AC3: retries are bounded (no infinite spin) and the skip is announced.
@test "chorus-api unavailable: retries are bounded, run is skipped + announced" {
  run env API_URL="$DEAD_API" DB_PATH="$TEST_DB" CHORUS_ROOT="$TEST_ROOT" \
    HEALTH_RETRY_MAX=2 HEALTH_RETRY_DELAY=0 bash "$SCRIPT"
  [[ "$output" == *"unavailable"* ]]
  [[ "$output" == *"skip"* ]]
}

# AC1 (resume): the health gate breaks out and proceeds the moment chorus-api
# answers — proven by attempt-count: a stub that 200s on /health lets the run pass
# the gate (it then exits on the empty crawl, NOT on the api gate).
@test "chorus-api reachable: passes the health gate (no skip message)" {
  # minimal stub server: always 200 on any path
  python3 -c "
import http.server, socketserver, threading, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self): self.send_response(200); self.end_headers(); self.wfile.write(b'{}')
    def log_message(self,*a): pass
with socketserver.TCPServer(('127.0.0.1', 8771), H) as s:
    open('$TEST_HOME/port.ready','w').close()
    s.serve_forever()
" &
  STUB_PID=$!
  for _ in $(seq 1 50); do [ -f "$TEST_HOME/port.ready" ] && break; sleep 0.05; done

  run env API_URL="http://127.0.0.1:8771" DB_PATH="$TEST_DB" CHORUS_ROOT="$TEST_ROOT" \
    HEALTH_RETRY_MAX=2 HEALTH_RETRY_DELAY=0 bash "$SCRIPT" notadomain
  kill "$STUB_PID" 2>/dev/null || true

  # it got PAST the api gate — no "unavailable/skip" message
  [[ "$output" != *"unavailable after"* ]]
}
