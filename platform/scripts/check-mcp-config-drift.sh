#!/usr/bin/env bash
# check-mcp-config-drift.sh (#4149) — the role MCP configs on disk must be
# exactly what gen-role-mcp.sh emits today. Red when they are not.
#
# This is the gate #4149 waived ("no test added for config text") and the
# waiver is why nobody noticed that every role had lost its Grafana/Loki
# tools. Landed is not running: a config can be correct in git and wrong on
# disk, and only a comparison of the two can tell you which.
#
# Usage: check-mcp-config-drift.sh [ROOT]   (default: the repo this lives in)
# Exit 0 = no drift. Exit 1 = drift, with the offending role named.
set -uo pipefail

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ROOT="$(cd "$ROOT" && pwd)"
GEN="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/gen-role-mcp.sh"
ROLES=(wren silas kade)

REF="$(mktemp -d)"
trap 'rm -rf "$REF"' EXIT

if ! GEN_MCP_WRITE_ROOT="$REF" bash "$GEN" >/dev/null 2>&1; then
  echo "FAIL: gen-role-mcp.sh did not run — cannot establish the reference config"
  exit 1
fi

drift=0
for role in "${ROLES[@]}"; do
  live="$ROOT/roles/$role/.mcp.json"
  ref="$REF/roles/$role/.mcp.json"

  if [ ! -f "$live" ]; then
    echo "FAIL: $role — no .mcp.json on disk; the role boots with no MCP tools at all"
    drift=1
    continue
  fi

  # Compare the server SET first: that is the failure Jeff feels (a tool is
  # missing from the session), and it names the loss plainly.
  live_srv="$(python3 -c "import json,sys;print(','.join(sorted(json.load(open(sys.argv[1]))['mcpServers'])))" "$live" 2>/dev/null)"
  ref_srv="$(python3 -c "import json,sys;print(','.join(sorted(json.load(open(sys.argv[1]))['mcpServers'])))" "$ref" 2>/dev/null)"

  if [ -z "$live_srv" ]; then
    echo "FAIL: $role — .mcp.json is unparseable; the role boots with no MCP tools"
    drift=1
    continue
  fi

  if [ "$live_srv" != "$ref_srv" ]; then
    echo "FAIL: $role — servers on disk [$live_srv] but the generator emits [$ref_srv]"
    drift=1
    continue
  fi

  # Same servers, different wiring (a stale binary path, a changed URL).
  if ! diff -q <(python3 -c "import json,sys;print(json.dumps(json.load(open(sys.argv[1])),sort_keys=True,indent=1))" "$live") \
               <(python3 -c "import json,sys;print(json.dumps(json.load(open(sys.argv[1])),sort_keys=True,indent=1))" "$ref") >/dev/null; then
    echo "FAIL: $role — same servers but the wiring differs from the generator's output"
    drift=1
    continue
  fi

  echo "PASS: $role — [$live_srv]"
done

if [ "$drift" -ne 0 ]; then
  echo "RED — role MCP config has drifted from gen-role-mcp.sh. Regenerate and commit."
  exit 1
fi
echo "GREEN — all role MCP configs match the generator"
