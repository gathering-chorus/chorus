#!/usr/bin/env bats
# @test-type: unit — signal:api is fixture-data (a fake credential minter; no service is called)
# @domain: identity
# 4515-mcp-headers.bats — #4515 AC2. What Jeff sees: every role's session
# reaches its tools through the runner with its own verified credential, and a
# session with no credential fails to connect instead of calling as nobody.

bats_require_minimum_version 1.5.0

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
HELPER="$REPO/platform/scripts/chorus-mcp-headers"

setup() {
  MINT="$BATS_TEST_TMPDIR/mint"
  printf '#!/bin/sh\n[ "$1" = "silas" ] || exit 3\necho fake-cred-for-$1\n' > "$MINT"
  chmod +x "$MINT"
  export CHORUS_TOKEN_BIN="$MINT"
}

@test "a role gets its own credential as an Authorization header" {
  run "$HELPER" silas
  [ "$status" -eq 0 ] || return 1
  [ "$output" = '{"Authorization": "Bearer fake-cred-for-silas"}' ] || return 1
}

@test "NEGATIVE PROOF: no credential exits non-zero and prints nothing on stdout" {
  run --separate-stderr "$HELPER" kade
  [ "$status" -ne 0 ] || return 1
  [ -z "$output" ] || return 1
}

@test "NEGATIVE PROOF: no role at all is refused" {
  run --separate-stderr env -u CHORUS_ROLE "$HELPER"
  [ "$status" -eq 2 ] || return 1
  [ -z "$output" ] || return 1
}

@test "every generated session config names the runner and the helper, never a role header" {
  for r in wren silas kade; do
    f="$REPO/roles/$r/.mcp.json"
    python3 -c 'import json,sys; s=json.load(open(sys.argv[1]))["mcpServers"]["chorus-api"]; assert s["url"]=="http://localhost:3342/mcp", s; assert s["headersHelper"].endswith("chorus-mcp-headers "+sys.argv[2]), s; assert "headers" not in s, s' "$f" "$r" || return 1
  done
}
