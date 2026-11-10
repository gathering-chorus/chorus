#!/usr/bin/env bats
# @test-type: integration
# @domain: cicd
# #4773 AC5: Negative proof: a deployment with mismatched or missing manifest is rejected by borg-prove.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TEST_DIR="$(mktemp -d)"
  export HOME="$TEST_DIR"
  export CHORUS_HOME="$ROOT"
  export CHORUS_BIN_DIR="$TEST_DIR/.chorus/bin"
  mkdir -p "$CHORUS_BIN_DIR"
  BORG_PROVE="$ROOT/platform/services/borg-prove/target/debug/borg-prove"
}

teardown() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TEST_DIR"
}

@test "4773 AC5 NEGATIVE PROOF: missing manifest is rejected by borg-prove" {
  MOCK_PORT=33777
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  # Stub door returns empty data (no deployment row for this card)
  python3 -c "
import http.server, socketserver, sys

class EmptyHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '{\"apiVersion\": \"v1\", \"kind\": \"Deployment\", \"data\": []}'
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), EmptyHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  run "$BORG_PROVE" manifest 4773 tool_b
  [ "$status" -ne 0 ]
  [[ "$output" =~ "missing deployment manifest" ]] || [[ "$output" =~ "0 Deployment rows" ]]
}

@test "4773 AC5 NEGATIVE PROOF: mismatched cdhash is rejected by borg-prove" {
  MOCK_PORT=33778
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  # Stub door returns manifest with cdhash=expected_hash_123
  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '''{
            \"apiVersion\": \"v1\",
            \"kind\": \"Deployment\",
            \"data\": [
                {
                    \"name\": \"dep-4773-tool_b-1731170000\",
                    \"cardId\": \"4773\",
                    \"deployedUnit\": \"tool_b\",
                    \"deployedCommit\": \"abc1234\",
                    \"cdhash\": \"expected_hash_123\",
                    \"sha256\": \"sha_xyz\",
                    \"targetMachine\": \"library\"
                }
            ]
        }'''
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), StubHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  mkdir -p "$TEST_DIR/bin"
  cat <<'EOF' > "$TEST_DIR/bin/codesign"
#!/bin/bash
target="$3"
if [ "$(basename "$target")" != "tool_b" ]; then
  echo "unexpected binary: $target" >&2
  exit 1
fi
echo "CDHash=mismatched_running_hash_999" >&2
EOF
  chmod +x "$TEST_DIR/bin/codesign"

  run "$BORG_PROVE" manifest 4773 tool_b - "$TEST_DIR/bin/codesign"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "manifest verification failed" ]] || [[ "$output" =~ "mismatched" ]]
}

@test "4773 AC5 NEGATIVE PROOF: empty cdhash in manifest is rejected by borg-prove" {
  MOCK_PORT=33779
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  # Stub door returns manifest with empty cdhash
  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '''{
            \"apiVersion\": \"v1\",
            \"kind\": \"Deployment\",
            \"data\": [
                {
                    \"name\": \"dep-4773-tool_b-1731170000\",
                    \"cardId\": \"4773\",
                    \"deployedUnit\": \"tool_b\",
                    \"deployedCommit\": \"abc1234\",
                    \"cdhash\": \"\",
                    \"sha256\": \"sha_xyz\",
                    \"targetMachine\": \"library\"
                }
            ]
        }'''
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), StubHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  run "$BORG_PROVE" manifest 4773 tool_b
  [ "$status" -ne 0 ]
  [[ "$output" =~ "empty cdhash" ]] || return 1
}

@test "4773 AC5 NEGATIVE PROOF: unreachable door is unmeasured and rejected by borg-prove" {
  export CHORUS_DOOR="http://127.0.0.1:39999"
  run "$BORG_PROVE" manifest 4773 tool_b
  [ "$status" -ne 0 ]
  [[ "$output" =~ "door did not answer" ]] || [[ "$output" =~ "unmeasured" ]]
}

@test "4773 AC5 NEGATIVE PROOF: unreadable deploy log is unmeasured and rejected by borg-prove" {
  MOCK_PORT=33780
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '{\"apiVersion\": \"v1\", \"kind\": \"Deployment\", \"data\": []}'
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), StubHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  run "$BORG_PROVE" manifest 4773 "-" "/nonexistent/path/deploy.jsonl"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "unreadable" ]] || [[ "$output" =~ "REFUSED" ]]
}

@test "4773 AC5 NEGATIVE PROOF: unset CHORUS_BIN is unmeasured naming CHORUS_BIN" {
  MOCK_PORT=33782
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '''{
            \"apiVersion\": \"v1\",
            \"kind\": \"Deployment\",
            \"data\": [
                {
                    \"name\": \"dep-4773-tool_b-1731170000\",
                    \"cardId\": \"4773\",
                    \"deployedUnit\": \"tool_b\",
                    \"deployedCommit\": \"abc1234\",
                    \"cdhash\": \"expected_hash_123\",
                    \"sha256\": \"sha_xyz\",
                    \"targetMachine\": \"library\"
                }
            ]
        }'''
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), StubHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  # Unset both CHORUS_BIN and CHORUS_BIN_DIR
  env -u CHORUS_BIN -u CHORUS_BIN_DIR "$BORG_PROVE" manifest 4773 tool_b 2>"$TEST_DIR/err.out" || STATUS=$?
  [ "$STATUS" -ne 0 ]
  run cat "$TEST_DIR/err.out"
  [[ "$output" =~ "CHORUS_BIN environment variable is not set" ]] || return 1
}

@test "4773 AC5 NEGATIVE PROOF: wrapper-only path (unsigned script without -bin) cdhash cannot be read and reads Not met" {
  MOCK_PORT=33783
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"
  export CHORUS_BIN="$CHORUS_BIN_DIR"

  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '''{
            \"apiVersion\": \"v1\",
            \"kind\": \"Deployment\",
            \"data\": [
                {
                    \"name\": \"dep-4773-tool_b-1731170000\",
                    \"cardId\": \"4773\",
                    \"deployedUnit\": \"tool_b\",
                    \"deployedCommit\": \"abc1234\",
                    \"cdhash\": \"expected_hash_123\",
                    \"sha256\": \"sha_xyz\",
                    \"targetMachine\": \"library\"
                }
            ]
        }'''
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body.encode('utf-8'))

    def log_message(self, format, *args):
        pass

socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), StubHandler)
httpd.serve_forever()
" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  # Place an unsigned shell script at $CHORUS_BIN_DIR/tool_b, and no tool_b-bin
  cat <<'EOF' > "$CHORUS_BIN_DIR/tool_b"
#!/bin/sh
echo "wrapper script only"
EOF
  chmod +x "$CHORUS_BIN_DIR/tool_b"
  rm -f "$CHORUS_BIN_DIR/tool_b-bin"

  run "$BORG_PROVE" manifest 4773 tool_b
  [ "$status" -ne 0 ]
  [[ "$output" =~ "cdhash cannot be read" ]] || [[ "$output" =~ "REFUSED" ]] || return 1
}
