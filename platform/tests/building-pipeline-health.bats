#!/usr/bin/env bats
# Tests for building-pipeline-health (#2870, hardened #3119).
#
# What Jeff sees: the deploy-chain health check went green 4-5 cards running
# while never once seeing a real /acp — it queried job="chorus-api" but
# chorus_acp.completed lands under job=platform-chorus, found 0, printed
# "idle; skipping", and exited 0. A green that passes by finding nothing.
#
# The load-bearing test here is "reports the acp, not idle" against a
# QUERY-AWARE mock Loki: the mock returns the acp for {job=~".+"} but EMPTY
# for {job="chorus-api"}, reproducing the label drift exactly. So this test
# FAILS against the old hardcoded queries and PASSES against the fix — it
# cannot pass by finding nothing.

# Resolve the repo root from THIS test's own location — UNCONDITIONALLY, not
# via ${CHORUS_ROOT:-...}: a session already exports CHORUS_ROOT=canonical, so a
# `:-` default never fires and the test would silently verify canonical instead
# of the tree it lives in (the werk during dev, the checkout in CI). Testing the
# unchanged code while the fix sits unverified in the werk is the exact trap.
TEST_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="${TEST_ROOT}/platform/scripts/building-pipeline-health"

MOCK_DIR=""
MOCK_PID=""
MOCK_PORT=""

setup() {
  MOCK_DIR=$(mktemp -d)
  # Fresh port per test — a shared fixed port lets a dying mock from the
  # previous test race the new one and answer with the wrong PAIRED data.
  MOCK_PORT=$(( 20000 + RANDOM % 5000 ))
}

teardown() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
    wait "$MOCK_PID" 2>/dev/null || true
  fi
  [ -n "$MOCK_DIR" ] && rm -rf "$MOCK_DIR"
}

# Query-aware Loki mock. Reads the LogQL `query` param and emits canned
# query_range JSON. Key behaviors:
#   - any query pinned to job="chorus-api"  -> empty result (models drift)
#   - query for release-trigger.completed   -> trigger event (trace T1)
#   - query for chorus_acp.completed        -> acp event (trace T1)
#   - query for deploy.completed            -> pipeline run
# PAIRED=1 (default) emits trigger+deploy so a fixed run is a clean PASS;
# PAIRED=0 emits only the acp so a fixed run flags unpaired (exit 1).
write_mock() {
  cat > "${MOCK_DIR}/loki_mock.py" <<'PY'
import os, sys, json, datetime
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs, unquote

PAIRED = os.environ.get("PAIRED", "1") == "1"
NOW = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S+00:00")
TRACE = "test-trace-T1"

def stream(line):
    return {"data": {"result": [{"stream": {}, "values": [["1700000000000000000", json.dumps(line)]]}]}}

EMPTY = {"data": {"result": []}}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        q = unquote(parse_qs(urlparse(self.path).query).get("query", [""])[0])
        # Drift: events do NOT live under job="chorus-api". The buggy code pins
        # that label and must get nothing.
        if 'job="chorus-api"' in q:
            body = EMPTY
        elif "release-trigger.completed" in q:
            body = stream({"event": "chorus_acp.release-trigger.completed", "ts": NOW, "trace_id": TRACE}) if PAIRED else EMPTY
        elif "chorus_acp.completed" in q:
            body = stream({"event": "chorus_acp.completed", "ts": NOW, "card_id": 9999, "role": "silas", "trace_id": TRACE})
        elif "deploy.completed" in q:
            body = stream({"event": "deploy.completed", "ts": NOW}) if PAIRED else EMPTY
        else:
            body = EMPTY
        payload = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
}

start_mock() {
  write_mock
  ( exec python3 "${MOCK_DIR}/loki_mock.py" "$MOCK_PORT" >/dev/null 2>&1 ) &
  MOCK_PID=$!
  for _ in $(seq 1 30); do
    curl -sf "http://localhost:${MOCK_PORT}/loki/api/v1/query_range?query=x" >/dev/null 2>&1 && return 0
    sleep 0.1
  done
  echo "mock loki failed to start on $MOCK_PORT" >&2
  return 1
}

run_health() {
  LOKI_URL="http://localhost:${MOCK_PORT}" \
    PIPELINE_LOG="${MOCK_DIR}/pipeline.log" \
    bash "$SCRIPT" "$@"
}

@test "script exists and is executable" {
  [ -x "$SCRIPT" ]
}

# The load-bearing test: an acp exists in Loki (under a non-chorus-api job),
# so the report MUST NOT say idle. RED against the old job="chorus-api" hardcode.
@test "sees a real /acp and does not pass by reporting idle" {
  export PAIRED=1
  start_mock
  run run_health
  # Single decisive assertion (bats honors only the LAST command's exit): the
  # 'release-trigger pairing:' line prints ONLY when total>0; the idle path
  # never emits it. A weaker `!= *idle*` mid-test would be silently ignored,
  # and `== *chorus_acp.completed events*` is matched by the idle message too
  # ("no chorus_acp.completed events ...") — both would pass by not checking.
  [[ "$output" == *"release-trigger pairing:"* ]]
}

# Same condition, JSON shape — acp_count must be >0, never silently 0.
@test "json output reports acp_count >= 1 when an acp exists" {
  export PAIRED=1
  start_mock
  run run_health --json
  [ "$status" -eq 0 ]
  run python3 -c "import json,sys; d=json.loads('''$output'''); assert d['acp_count'] >= 1, d; assert d['unpaired_release_trigger']==0 and d['unpaired_pipeline_run']==0, d"
  [ "$status" -eq 0 ]
}

# An acp with no paired trigger/pipeline must FLAG (exit 1) — not pass.
@test "flags exit 1 when an acp has no paired release-trigger or pipeline run" {
  export PAIRED=0
  start_mock
  run run_health
  [ "$status" -eq 1 ]
  [[ "$output" != *"idle"* ]]
  [[ "$output" == *"missing"* ]]
}

# Genuine idle (no acp anywhere) is the ONLY case "idle; skipping" is allowed.
@test "true idle (no acp under any job) skips cleanly" {
  cat > "${MOCK_DIR}/loki_mock.py" <<'PY'
import sys, json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_GET(self):
        b=json.dumps({"data":{"result":[]}}).encode()
        self.send_response(200); self.send_header("Content-Length",str(len(b))); self.end_headers(); self.wfile.write(b)
HTTPServer(("127.0.0.1",int(sys.argv[1])),H).serve_forever()
PY
  ( exec python3 "${MOCK_DIR}/loki_mock.py" "$MOCK_PORT" >/dev/null 2>&1 ) &
  MOCK_PID=$!
  for _ in $(seq 1 30); do curl -sf "http://localhost:${MOCK_PORT}/x" >/dev/null 2>&1 && break; sleep 0.1; done
  run run_health
  [ "$status" -eq 0 ]
  [[ "$output" == *"idle"* ]]
}
