#!/bin/bash
# chorus-mcp-wrapper.sh — launchd wrapper for chorus-mcp (#2997).
# Mirrors chorus-api-wrapper.sh shape: source nvm, run node against dist/main.js.

set -e

# Set up node from nvm
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"

# #3197 — source the ONE env file. CHORUS_ROOT/HOME/WERK_BASE/BIN come from here,
# the single source, and land in process.env so the node app reads them — instead
# of hand-copied plist vars that kept drifting.
source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"

# Resolve chorus-mcp location. Default canonical; override via CHORUS_MCP_DIR.
MCP_DIR="${CHORUS_MCP_DIR:-/Users/jeffbridwell/CascadeProjects/chorus/platform/mcp-server}"

cd "$MCP_DIR"
exec node dist/main.js
