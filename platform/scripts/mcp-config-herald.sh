#!/bin/bash
# mcp-config-herald.sh — emit a spine event + bridge advisory when .mcp.json changes
# #2475 thread 4 of 5
#
# Triggered by com.chorus.mcp-config-herald LaunchAgent (WatchPaths on
# /Users/jeffbridwell/CascadeProjects/chorus/.mcp.json). When the file changes,
# all role Claude Code sessions need to restart to pick up the new tools/list.
# Without this herald, a tool addition would silently fail to reach roles.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
MCP_CONFIG="${CHORUS_ROOT}/.mcp.json"
STATE_DIR="${HOME}/.chorus"
HASH_FILE="${STATE_DIR}/mcp-config-last-hash"
LOG_BIN="${CHORUS_ROOT}/platform/scripts/chorus-log"
BRIDGE_URL="http://localhost:3470/api/message"

mkdir -p "$STATE_DIR"

if [ ! -f "$MCP_CONFIG" ]; then
  exit 0  # No config = nothing to herald
fi

CURRENT_HASH=$(shasum -a 256 "$MCP_CONFIG" | awk '{print $1}')
LAST_HASH=$(cat "$HASH_FILE" 2>/dev/null || echo "")

if [ "$CURRENT_HASH" = "$LAST_HASH" ]; then
  exit 0  # No change
fi

# Persist new hash atomically
echo "$CURRENT_HASH" > "${HASH_FILE}.tmp" && mv "${HASH_FILE}.tmp" "$HASH_FILE"

# Spine event — drift visible to anyone reading the log
"$LOG_BIN" mcp.config.changed system \
  "hash=${CURRENT_HASH:0:16}" \
  "prev=${LAST_HASH:0:16}" 2>/dev/null || true

# First-run case: no prior hash, just record and skip the loud advisory
if [ -z "$LAST_HASH" ]; then
  exit 0
fi

# Bridge advisory — Jeff sees it, all 3 roles see it via subscriber tail
curl -s --max-time 3 -X POST "$BRIDGE_URL" \
  -H 'Content-Type: application/json' \
  -d "{\"from\":\"silas\",\"text\":\"[mcp-config-herald] .mcp.json changed (hash ${CURRENT_HASH:0:12}). Roles need /reboot to pick up the new tools/list. Until restart, sessions are running on the previous tool surface.\"}" \
  >/dev/null || true
