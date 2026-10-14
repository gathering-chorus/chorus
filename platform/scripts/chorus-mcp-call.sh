#!/usr/bin/env bash
# chorus-mcp-call.sh — one tool call, fail-loud (#3236; through the runner since #4515).
#
# Same contract its callers have always had (3 launchd jobs, chat.sh, athena-land):
#   chorus-mcp-call.sh <role> <tool> <args-json>
#   exit 0 on tool success, non-zero on refusal or failure; tool output on stdout.
# Since #4515 there is no old MCP server to call: this hands the call to
# `chorus-runner call`, which runs the tool in-process from its Skill row, as
# <role>, with the same checks the runner's MCP side makes. No HTTP, no session.
# A "role" key in the JSON is dropped: who runs it is <role>, never an argument.

set -uo pipefail

ROLE="${1:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"
TOOL="${2:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"
ARGS="${3:?usage: chorus-mcp-call.sh <role> <tool> <args-json>}"
RUNNER="${CHORUS_RUNNER_BIN:-$(command -v chorus-runner || echo "$HOME/.chorus/bin/chorus-runner")}"

exec python3 - "$RUNNER" "$ROLE" "$TOOL" "$ARGS" <<'PY'
import json, os, sys
runner, role, tool, raw = sys.argv[1:5]
try:
    args = json.loads(raw)
except ValueError as e:
    sys.exit(f"chorus-mcp-call: args are not JSON ({e}): {raw}")
if not isinstance(args, dict):
    sys.exit("chorus-mcp-call: args must be a JSON object")
args.pop("role", None)
def text(v):
    return v if isinstance(v, str) else json.dumps(v)
pairs = []
for k, v in args.items():
    for item in (v if isinstance(v, list) else [v]):
        pairs.append(f"{k}={text(item)}")
os.environ["ROLE"] = role
os.execv(runner, [runner, "call", tool] + pairs)
PY
