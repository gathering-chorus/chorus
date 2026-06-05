#!/usr/bin/env bats
# werk-acp-retired.bats — #3219
#
# Asserts that the werk-acp composition verb, its MCP thin-skin (chorus_acp),
# and its test script have been retired from disk + the server. Background:
# #3175 made werk-accept atomic finalize-only and #3237 made it the go-signal,
# so the flat sequence ends with accept→finalize — werk-acp (the v2 composition
# verb) is obsolete but was still callable by habit (Silas's #3211 nit). This
# card deletes it so it cannot resurrect. event-name references (Loki queries
# for chorus_acp.completed in ops/health scripts) are historical and explicitly
# out of scope here — this gate only guards the verb + its callable surface.

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
SERVER="$CHORUS_ROOT/platform/mcp-server/src/server.ts"

@test "werk-acp crate is deleted from disk" {
  if [ -d "$CHORUS_ROOT/platform/services/werk-acp" ]; then
    echo "platform/services/werk-acp still exists — must be deleted (#3219)"
    false
  fi
}

@test "no chorus_acp tool definition or switch case in server.ts" {
  matches=$(grep -n "name: 'chorus_acp'\|case 'chorus_acp'\|ACP_TOOL_DEF" "$SERVER" 2>/dev/null \
    | grep -v -E '^\s*//|retired|removed|deprecated|#3219' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found chorus_acp tool surface in server.ts:"
    echo "$matches"
    false
  fi
}

@test "no werk-acp in the executeWerkVerb verb union type" {
  matches=$(grep -n "'werk-acp'" "$SERVER" 2>/dev/null \
    | grep -v -E '^\s*//|retired|removed|deprecated|#3219' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found 'werk-acp' verb reference in server.ts:"
    echo "$matches"
    false
  fi
}

@test "test-acp.sh is deleted" {
  if [ -f "$CHORUS_ROOT/platform/scripts/test-acp.sh" ]; then
    echo "platform/scripts/test-acp.sh still exists — obsolete, must be deleted (#3219)"
    false
  fi
}
