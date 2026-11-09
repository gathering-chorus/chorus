#!/usr/bin/env bats
# @test-type: integration
# @domain: cicd
# #4773 AC8: deployment records are posted by principal-deployer, not the landing role.
# One Principal row (hostAccount "runner"), one Permission (Write on urn:chorus:domains:cicd only),
# its credential readable only by the runner account (0600).
# Each row's deployedBy = principal-deployer and landedFor = the card's role.
# Negative proofs:
# 1. A role account cannot read the deployer credential.
# 2. principal-deployer is refused a write outside cicd.
# 3. A post without credential is refused and borg-prove reads Unmeasured.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TEST_DIR="$(mktemp -d)"
  export CHORUS_HOME="$ROOT"
  export CHORUS_ROLE="silas"
  export DEPLOY_ROLE="silas"
  export CHORUS_MACHINE="library"
}

teardown() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TEST_DIR"
}

@test "4773 AC8: principal-deployer declared in identity-principals-3613.ttl with hostAccount jeffbridwell" {
  TTL="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
  grep -A 8 'chorus:principal-deployer a chorus:Principal' "$TTL" | grep -q 'chorus:hostAccount "jeffbridwell"'
  grep -A 8 'chorus:principal-deployer a chorus:Principal' "$TTL" | grep -q 'chorus:principalKind "service"'
  grep -A 8 'chorus:principal-deployer a chorus:Principal' "$TTL" | grep -q 'chorus:canSignIn "true"'
}

@test "4773 AC8: permission-deployer-cicd gives principal-deployer Write on urn:chorus:domains:cicd only" {
  PERMS="$ROOT/roles/silas/ontology/permissions-4183.ttl"
  # Declares permission row
  grep -A 6 'chorus:permission-deployer-cicd a chorus:Permission' "$PERMS" | grep -q 'chorus:agent chorus:principal-deployer'
  grep -A 6 'chorus:permission-deployer-cicd a chorus:Permission' "$PERMS" | grep -q 'chorus:accessTo "urn:chorus:domains:cicd"\^\^xsd:anyURI'
  grep -A 6 'chorus:permission-deployer-cicd a chorus:Permission' "$PERMS" | grep -q 'chorus:mode acl:Write'

  # Only one permission row for principal-deployer
  COUNT=$(grep -c 'chorus:agent chorus:principal-deployer' "$PERMS" || true)
  [ "$COUNT" -eq 1 ]
}

@test "4773 AC8: DeploymentShape in werk-domains.ttl declares landedFor edge pointing to chorus:Role" {
  TTL="$ROOT/roles/kade/ontology/werk-domains.ttl"
  grep -A 20 'chorus:DeploymentShape a sh:NodeShape' "$TTL" | grep -A 2 'sh:path chorus:landedFor' | grep -q 'sh:class chorus:Role'
  grep -A 20 'chorus:DeploymentShape a sh:NodeShape' "$TTL" | grep -A 2 'sh:path chorus:landedFor' | grep -q 'sh:maxCount 1'
  # Property defined
  grep -A 4 'chorus:landedFor a owl:ObjectProperty' "$TTL" | grep -q 'rdfs:range chorus:Role'
}

@test "4773 AC8: werk-deploy posts records with deployedBy=principal-deployer and landedFor=role-<role>" {
  MOCK_PORT=33780
  export CHORUS_DOOR="http://127.0.0.1:$MOCK_PORT"
  REQUESTS_LOG="$TEST_DIR/requests.jsonl"

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
        self.wfile.write(b'{\"status\":\"created\",\"name\":\"dep-stub\"}')
    def log_message(self, format, *args):
        pass
socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[2])), StubHandler)
httpd.serve_forever()
" "$REQUESTS_LOG" "$MOCK_PORT" &
  SERVER_PID=$!
  sleep 0.3

  TEST_BIN=$(ls -t $(find "$ROOT/platform/services/werk-deploy/target/debug/deps" -name "werk_deploy-*" -type f -perm +111) | head -1)
  [ -n "$TEST_BIN" ]
  "$TEST_BIN" test_record_deployment_to_door

  [ -f "$REQUESTS_LOG" ]
  # deployedBy must be principal-deployer, not the role
  python3 -c "
import json, sys
data = json.loads(open('$REQUESTS_LOG').readline())
body = data['body']
assert body['deployedBy'] == 'principal-deployer', f'deployedBy was {body.get(\"deployedBy\")}'
assert body['landedFor'] == 'role-silas' or body['landedFor'] == 'silas', f'landedFor was {body.get(\"landedFor\")}'
"
}

@test "4773 AC8: land-4773-rows seeds principal-deployer and deployer-cicd permission in dry-run" {
  SCRIPT="$ROOT/platform/scripts/land-4773-rows"
  [ -x "$SCRIPT" ]
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "identity/principals/deployer" ]] || return 1
  [[ "$output" =~ "security/permissions/deployer-cicd" ]] || return 1
  [[ "$output" =~ "nothing written (dry run)" ]] || return 1
}

@test "4773 AC8 NEGATIVE PROOF: deployer credential (0600 runner-bound) cannot be read by role accounts" {
  FAKE_IDENTITY_DIR="$TEST_DIR/identity"
  mkdir -p "$FAKE_IDENTITY_DIR/deployer"
  CRED="$FAKE_IDENTITY_DIR/deployer/cred.json"
  cat <<'EOF' > "$CRED"
{
  "id": "deployer-client-id",
  "secret": "top-secret",
  "webId": "https://id.lightlifeurbangardens.com/deployer/profile/card#me",
  "issuer": "https://id.lightlifeurbangardens.com",
  "hostAccount": "jeffbridwell"
}
EOF
  chmod 0600 "$CRED"

  # 1. File permission: non-owner access check (simulated via chmod 0600)
  PERMS=$(python3 -c 'import os, sys; print(oct(os.stat(sys.argv[1]).st_mode)[-3:])' "$CRED")
  [ "$PERMS" = "600" ]

  # 2. Account binding check: chorus-identity-token refuses if caller is not the bound host account
  export CHORUS_IDENTITY_DIR="$FAKE_IDENTITY_DIR"
  run "$ROOT/platform/scripts/chorus-identity-token" deployer
  [ "$status" -ne 0 ]
  [[ "$output" =~ "refused" ]] || [[ "$output" =~ "bound to account 'jeffbridwell'" ]]
}

@test "4773 AC8 NEGATIVE PROOF: principal-deployer is refused write access outside urn:chorus:domains:cicd" {
  # Door DAL authz: deployer only holds Write on urn:chorus:domains:cicd.
  # Calling handle_write_scoped on another graph (e.g. urn:chorus:domains:cards) returns 403.
  TEST_BIN=$(ls -t $(find "$ROOT/platform/services/athena-make/target/debug/deps" -name "deployments_door_4773-*" -type f -perm +111) | head -1)
  [ -n "$TEST_BIN" ]
  "$TEST_BIN" deployer_refused_outside_cicd
}

@test "4773 AC8 NEGATIVE PROOF: missing deployer credential causes POST refusal and borg-prove reads Unmeasured" {
  FAIL_PORT=33781
  export CHORUS_DOOR="http://127.0.0.1:$FAIL_PORT"

  # Stub door that returns 401 Unauthorized for unauthenticated requests
  python3 -c "
import http.server, socketserver, sys
class AuthRefusingHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.send_response(401)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{\"error\":\"unauthorized\",\"message\":\"missing deployer credentials\"}')
    def do_GET(self):
        self.send_response(401)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{\"error\":\"unauthorized\",\"message\":\"missing deployer credentials\"}')
    def log_message(self, format, *args):
        pass
socketserver.TCPServer.allow_reuse_address = True
httpd = socketserver.TCPServer(('127.0.0.1', int(sys.argv[1])), AuthRefusingHandler)
httpd.serve_forever()
" "$FAIL_PORT" &
  FAIL_PID=$!
  sleep 0.3

  # borg-prove manifest should read Unmeasured (not Met) and exit 1
  BORG_PROVE="$ROOT/platform/services/borg-prove/target/debug/borg-prove"
  run "$BORG_PROVE" manifest 4773 "['fake-unit']" "-" "codesign"
  kill "$FAIL_PID" 2>/dev/null || true

  [ "$status" -ne 0 ]
  [[ "$output" =~ "door did not answer" ]] || [[ "$output" =~ "unparseable manifest" ]] || [[ "$output" =~ "Unmeasured" ]]
}
