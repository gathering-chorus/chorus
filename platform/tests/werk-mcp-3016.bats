#!/usr/bin/env bats
# #3016 — werk chorus-mcp daemon: teardown (chorus-werk) + .mcp.json endpoint
# templating. Companion to chorus-env-setup.bats (CHORUS_MCP_PORT resolution)
# and the chorus-deploy --target werk chorus-mcp path.

SCRIPT_DIR="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)"
ROOT="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../.." && pwd)"
WERK_SH="$SCRIPT_DIR/chorus-werk"

# Source chorus-werk for its functions. `help` is a non-exiting subcommand, so
# `main "$@"` returns cleanly and teardown_werk_mcp is defined.
load_werk() { source "$WERK_SH" help >/dev/null 2>&1; }

@test "teardown_werk_mcp boots out the agent + removes plist + drops marker" {
  TMPHOME="$(mktemp -d)"
  WERK="$(mktemp -d)"
  mkdir -p "$WERK/.werk-mcp" "$TMPHOME/Library/LaunchAgents"
  echo "com.chorus.mcp.werk.testrole" > "$WERK/.werk-mcp/label"
  : > "$WERK/.werk-mcp/active"
  touch "$TMPHOME/Library/LaunchAgents/com.chorus.mcp.werk.testrole.plist"
  ( HOME="$TMPHOME"; load_werk; teardown_werk_mcp "$WERK" )
  [ ! -d "$WERK/.werk-mcp" ]
  [ ! -f "$TMPHOME/Library/LaunchAgents/com.chorus.mcp.werk.testrole.plist" ]
  rm -rf "$TMPHOME" "$WERK"
}

@test "teardown_werk_mcp is idempotent — no marker is a silent no-op (exit 0)" {
  WERK="$(mktemp -d)"
  run bash -c "source '$WERK_SH' help >/dev/null 2>&1; teardown_werk_mcp '$WERK'"
  [ "$status" -eq 0 ]
  rm -rf "$WERK"
}

@test ".mcp.json url is templated with CHORUS_MCP_PORT and a 3341 fallback" {
  url="$(python3 -c "import json;print(json.load(open('$ROOT/.mcp.json'))['mcpServers']['chorus-api']['url'])")"
  [[ "$url" == *'${CHORUS_MCP_PORT:-3341}'* ]]
}

@test ".mcp.json remains valid JSON after templating" {
  python3 -c "import json;json.load(open('$ROOT/.mcp.json'))"
}
