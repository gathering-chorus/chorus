#!/usr/bin/env bash
# gen-role-mcp.sh (#3120) — generate every role's .mcp.json from ONE template so
# the team's MCP tool config (chorus-api + cclsp + ast-grep) can't drift and no
# role gets skipped. The role name is the ONLY per-role variable. Mirrors
# claudemd-gen.sh's "generate, don't hand-edit" contract.
#
# Why generated, not hand-edited: the LSP/AST rollout clustered precisely because
# configs were hand-patched (kade had the tools, wren/silas didn't; one boot-time
# git-fetch defect rode along). One source + one variable removes the whole class.
#
# ast-grep points at the LOCAL, pinned binary (resolved at gen time) — never a
# `uvx --from git+...` boot-time fetch.
set -euo pipefail

# WRITE_ROOT = the repo this script lives in (werk when building, canonical when
# regenerating in place). Resolve from the script's own location, NOT $CHORUS_HOME,
# so running the werk's copy writes into the werk.
WRITE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# RUNTIME_HOME = where role SESSIONS actually run from (canonical). The baked
# absolute paths (cclsp config) must point here regardless of which tree generated
# the file. Overridable for other machines.
RUNTIME_HOME="${CHORUS_CANONICAL_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"
CCLSP_CONFIG="$RUNTIME_HOME/cclsp.json"

# ast-grep MCP server: the local, pinned binary (installed via `uv tool install`
# at a fixed sha). Resolve it; fall back to the conventional uv tool bin path.
ASTGREP="$(command -v ast-grep-server || echo "$HOME/.local/bin/ast-grep-server")"

ROLES=(wren silas kade)

for role in "${ROLES[@]}"; do
  out="$WRITE_ROOT/roles/$role/.mcp.json"
  mkdir -p "$(dirname "$out")"
  cat > "$out" <<JSON
{
  "\$schema": "https://schemas.modelcontextprotocol.io/mcp.schema.json",
  "mcpServers": {
    "chorus-api": {
      "type": "http",
      "url": "http://localhost:3341/mcp",
      "headers": { "X-Chorus-Role": "$role" }
    },
    "cclsp": {
      "command": "cclsp",
      "env": { "CCLSP_CONFIG_PATH": "$CCLSP_CONFIG" }
    },
    "ast-grep": {
      "command": "$ASTGREP"
    }
  }
}
JSON
  echo "generated $out (role=$role)"
done
