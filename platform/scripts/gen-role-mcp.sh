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

# cclsp MCP server (#3125): cclsp is a Node script (shebang `#!/usr/bin/env
# node`), so a bare `command: "cclsp"` needs BOTH cclsp AND node resolvable on
# PATH. A VS-Code-hosted session doesn't carry nvm's PATH, so both vanish and
# cclsp silently fails to load (Wren's LSP block, 2026-05-29). Bake both
# absolute paths at gen time — same approach as ASTGREP. Resolve via PATH (gen
# normally runs in an nvm-active shell); fall back to the current pinned nvm
# node version if gen runs PATH-limited. (Fallback is coupled to the nvm node
# version — `command -v` is the primary path and self-heals on node upgrades.)
NODE_BIN="$(command -v node || echo "$HOME/.nvm/versions/node/v20.20.2/bin/node")"
CCLSP_BIN="$(command -v cclsp || echo "$HOME/.nvm/versions/node/v20.20.2/bin/cclsp")"

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
      "command": "$NODE_BIN",
      "args": ["$CCLSP_BIN"],
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
