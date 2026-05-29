#!/usr/bin/env bats
# 3125-cclsp-abs-path — gen-role-mcp.sh must bake cclsp's launch via ABSOLUTE
# node + cclsp paths, never a bare `command: "cclsp"`. cclsp is a Node script
# (#!/usr/bin/env node), so bare resolution depends on nvm's PATH, which a
# VS-Code-hosted session doesn't carry → cclsp silently fails to load (the
# 2026-05-29 Wren LSP block). ast-grep already bakes absolute; cclsp must too.
#
# Tree-relative on purpose: tests the tree it runs in (the werk now, canonical
# post-merge), not a hardcoded canonical that wouldn't carry the fix yet.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
GEN="$REPO_ROOT/platform/scripts/gen-role-mcp.sh"

@test "gen-role-mcp.sh resolves absolute node + cclsp paths" {
  grep -qE 'NODE_BIN="\$\(command -v node' "$GEN"
  grep -qE 'CCLSP_BIN="\$\(command -v cclsp' "$GEN"
}

@test "cclsp MCP block launches via absolute node with cclsp as arg" {
  grep -qE '"command": "\$NODE_BIN"' "$GEN"
  grep -qE '"args": \["\$CCLSP_BIN"\]' "$GEN"
}

@test "no bare command:cclsp regression (the nvm-PATH break)" {
  # The exact rot we are fixing must not reappear.
  ! grep -qE '"command": "cclsp"' "$GEN"
}

@test "running the generator emits an absolute cclsp command" {
  run bash "$GEN"
  [ "$status" -eq 0 ]
  run python3 -c "import json; print(json.load(open('$REPO_ROOT/roles/wren/.mcp.json'))['mcpServers']['cclsp']['command'])"
  [ "$status" -eq 0 ]
  # Command must be an absolute path (starts with /), not a bare name.
  [[ "$output" == /* ]]
}
