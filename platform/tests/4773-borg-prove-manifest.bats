#!/usr/bin/env bats
# @test-type: integration
# @domain: cicd
# #4773 AC4: borg-prove reads the manifest on land and asserts prod running hashes match the manifest exactly.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TEST_DIR="$(mktemp -d)"
  export HOME="$TEST_DIR"
  export CHORUS_HOME="$ROOT"
  export CHORUS_BIN_DIR="$TEST_DIR/.chorus/bin"
  mkdir -p "$CHORUS_BIN_DIR"

  # Find an unused port for stub door
  MOCK_PORT=33776
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"

  # Python stub door returning a Deployment row for card 4773
  python3 -c "
import http.server, socketserver, sys

class StubHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = '''{
            \"apiVersion\": \"v1\",
            \"kind\": \"Deployment\",
            \"data\": [
                {
                    \"name\": \"dep-4773-tool_a-1731170000\",
                    \"cardId\": \"4773\",
                    \"deployedUnit\": \"tool_a\",
                    \"deployedCommit\": \"abc1234\",
                    \"cdhash\": \"mock_cdhash_good\",
                    \"sha256\": \"mock_sha_good\",
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
}

teardown() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TEST_DIR"
}

@test "4773 AC4: borg-prove asserts prod running hashes match the manifest" {
  mkdir -p "$TEST_DIR/bin"
  cat <<'EOF' > "$TEST_DIR/bin/codesign"
#!/bin/bash
target="$3"
if [ "$(basename "$target")" != "tool_a" ]; then
  echo "unexpected binary: $target" >&2
  exit 1
fi
echo "CDHash=mock_cdhash_good" >&2
EOF
  chmod +x "$TEST_DIR/bin/codesign"

  BORG_PROVE="$ROOT/platform/services/borg-prove/target/debug/borg-prove"
  run "$BORG_PROVE" manifest 4773 tool_a - "$TEST_DIR/bin/codesign"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "verified 1 deployment record" ]] || return 1
}

@test "4773 AC4: zero expected units passes when card deployed no binaries" {
  BORG_PROVE="$ROOT/platform/services/borg-prove/target/debug/borg-prove"
  run "$BORG_PROVE" manifest 4773 ""
  [ "$status" -eq 0 ]
  [[ "$output" =~ "no binaries deployed for this card" ]] || return 1
}
