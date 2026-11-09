#!/usr/bin/env bats
# @test-type: integration:api
# @domain: cicd
# #4773 AC3: werk-deploy / cicd-land posts the signed manifest as the final deployment step.
# Tests use a stub door — never hitting live prod :3360 or :3365.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TEST_DIR="$(mktemp -d)"
  export CHORUS_HOME="$ROOT"
  export CHORUS_ROLE="silas"
  export DEPLOY_ROLE="silas"
  export CHORUS_MACHINE="library"

  # Find an unused port for the stub door
  MOCK_PORT=33774
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"
  REQUESTS_LOG="$TEST_DIR/requests.jsonl"

  # Python stub door server
  python3 -c "
import http.server, socketserver, sys, json

requests_log = sys.argv[1]

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(length).decode('utf-8')
        with open(requests_log, 'a') as f:
            f.write(json.dumps({'path': self.path, 'body': json.loads(body)}) + '\n')
        self.send_response(201)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{\"status\":\"created\",\"name\":\"stub-dep\"}')

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[2])), StubHandler)
httpd.serve_forever()
" "$REQUESTS_LOG" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3
}

teardown() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TEST_DIR"
}

@test "4773 AC3 guard: test harness strictly points to stub door, never :3360 or :3365" {
  [ "$CHORUS_DOOR" != "http://localhost:3360" ]
  [ "$CHORUS_DOOR" != "http://127.0.0.1:3360" ]
  [ "$CHORUS_DOOR" != "http://localhost:3365" ]
  [ "$CHORUS_DOOR" != "http://127.0.0.1:3365" ]
  [[ "$CHORUS_DOOR" =~ "33774" ]] || return 1
}

@test "4773 AC3: post_deployment_manifest posts valid Deployment record to door" {
  TEST_BIN=$(find "$ROOT/platform/services/werk-deploy/target/debug/deps" -name "werk_deploy-*" -type f -perm +111 | head -1)
  "$TEST_BIN" test_post_deployment_manifest_to_stub_door
  [ -f "$REQUESTS_LOG" ]
  run grep '"cdhash"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
  run grep '"deployedCommit"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
  run grep '"sha256"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
  run grep '"targetMachine"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
  run grep '"deployedBy"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
}

@test "4773 AC3 rollback: posting rollback manifest includes rollbackOf reference" {
  TEST_BIN=$(find "$ROOT/platform/services/werk-deploy/target/debug/deps" -name "werk_deploy-*" -type f -perm +111 | head -1)
  "$TEST_BIN" test_post_rollback_manifest_to_stub_door
  [ -f "$REQUESTS_LOG" ]
  run grep '"rollbackOf"' "$REQUESTS_LOG"
  [ "$status" -eq 0 ]
}

@test "4773 AC3 NEGATIVE PROOF: rejected manifest (422) is caught as error and not marked success" {
  kill "$SERVER_PID" 2>/dev/null || true
  FAIL_PORT=33775
  export CHORUS_DOOR="http://127.0.0.1:$FAIL_PORT"

  python3 -c "
import http.server, socketserver, sys

class FailHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get('Content-Length', 0))
        _ = self.rfile.read(length)
        self.send_response(422)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{\"error\":\"validation\",\"message\":\"missing required cdhash\"}')

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), FailHandler)
httpd.serve_forever()
" "$FAIL_PORT" &
  FAIL_PID=$!
  sleep 0.3

  TEST_BIN=$(find "$ROOT/platform/services/werk-deploy/target/debug/deps" -name "werk_deploy-*" -type f -perm +111 | head -1)
  "$TEST_BIN" test_post_manifest_rejection_returns_error
  kill "$FAIL_PID" 2>/dev/null || true
}
