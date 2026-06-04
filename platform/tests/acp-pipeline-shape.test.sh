#!/usr/bin/env bash
# #3236: hermetic shape test for the collapsed act-orchestrated, MCP-toolchain
# pipeline. Encodes the design decision (Jeff, 2026-06-04): ONE orchestrator (act)
# + ONE toolchain (MCP). Replaces the two-orchestrator drift (werk-mcp.sh bash +
# acp.yml direct-binary) where a fix in one path (e.g. #3234 stop-before-accept)
# silently left the other path bugged.
#
# Shape/plumbing only — workflows exist and are wired to the invariants below. Live
# act-run behavior is integration-gated (needs chorus-api + real verbs), covered
# separately. Dependency-free by design (grep, no YAML lib): the existing
# ci-workflow-shape.test.sh leaned on node_modules/js-yaml which isn't installed in
# an ephemeral werk — a shape test must not need a parser to run.
#
# Invariants asserted (each maps to a card AC + an upstream fix the act path must carry):
#   acp.yml:
#     - workflow_dispatch trigger
#     - does NOT run werk-accept            (#3234 stop-before-accept — accept is the human's hand, DEC-048)
#     - has a canonical ff-sync step        (#3234 — script-only changes land live)
#     - "live" claim gated on sync success  (#3234 SYNC_OK — no merged≠live lie)
#     - NO standalone `git rebase` step     (#3186 — rebase happens inside werk-commit, under flock)
#     - verb calls go through MCP           (single toolchain — no bare verb-binary invocation)
#   demo.yml:
#     - exists, workflow_dispatch
#     - runs the demo (werk-demo)
#     - deploys to the werk slot (prove half)

set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
ACP="$REPO/.github/workflows/acp.yml"
DEMO="$REPO/.github/workflows/demo.yml"

PASSED=0
FAILED=0

assert() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"; PASSED=$((PASSED + 1))
  else
    echo "  FAIL: $label — expected='$expected' actual='$actual'"; FAILED=$((FAILED + 1))
  fi
}

# present "yes" if the ERE matches anywhere in the file, else "no".
present() { grep -Eq "$2" "$1" 2>/dev/null && echo yes || echo no; }
# absent "yes" if the ERE does NOT match anywhere in the file, else "no".
absent()  { grep -Eq "$2" "$1" 2>/dev/null && echo no  || echo yes; }

# ---------------------------------------------------------------- acp.yml
if [ -f "$ACP" ]; then
  assert "acp.yml exists" "yes" "yes"
else
  assert "acp.yml exists" "yes" "no"; echo "Aborting."; exit 1
fi

assert "acp.yml triggers on workflow_dispatch" "yes" \
  "$(present "$ACP" 'workflow_dispatch')"

# #3234: stop-before-accept. NO step may INVOKE werk-accept — but the workflow MUST
# still PRINT the accept command for the human. So we reject only real invocations
# (via the MCP helper, or as a bare line-start binary), not an echo'd instruction or
# a comment mentioning it.
assert "acp.yml does NOT invoke werk-accept (stop-before-accept #3234)" "yes" \
  "$(absent "$ACP" 'chorus-mcp-call\.sh[[:space:]]+[a-z]+[[:space:]]+werk-accept|^[[:space:]]*werk-accept[[:space:]]')"

# ...and it DOES print the accept command for the human to run (DEC-048 — the hand-off).
assert "acp.yml prints the accept command for the human (#3234/DEC-048)" "yes" \
  "$(present "$ACP" 'echo.*werk-accept')"

# #3234: canonical ff-sync present (script-only changes land live).
assert "acp.yml has canonical ff-sync step (#3234)" "yes" \
  "$(present "$ACP" 'merge[[:space:]]+--ff-only[[:space:]]+origin/main')"

# #3234: the "live" claim must be gated on the sync result (truth-conditional).
assert "acp.yml gates live-claim on sync (SYNC_OK #3234)" "yes" \
  "$(present "$ACP" 'SYNC_OK')"

# #3186: rebase happens inside werk-commit under flock — no standalone rebase step.
assert "acp.yml has NO standalone git rebase step (#3186)" "yes" \
  "$(absent "$ACP" 'git[[:space:]].*rebase[[:space:]]+origin/main')"

# Single toolchain: verbs invoked via the shared MCP helper (or tools/call), not bare.
assert "acp.yml calls verbs via MCP helper (single toolchain)" "yes" \
  "$(present "$ACP" 'chorus-mcp-call|tools/call')"

# No bare verb-binary invocation (line-start verb call, ignoring leading whitespace).
assert "acp.yml has no bare verb-binary invocation" "yes" \
  "$(absent "$ACP" '^[[:space:]]*(werk-commit|werk-push|werk-merge|werk-build|werk-deploy)[[:space:]]')"

# ---------------------------------------------------------------- demo.yml
if [ -f "$DEMO" ]; then
  assert "demo.yml exists" "yes" "yes"
  assert "demo.yml triggers on workflow_dispatch" "yes" \
    "$(present "$DEMO" 'workflow_dispatch')"
  assert "demo.yml runs the demo (werk-demo)" "yes" \
    "$(present "$DEMO" 'werk-demo')"
  assert "demo.yml deploys to the werk slot (target=werk)" "yes" \
    "$(present "$DEMO" 'werk')"
else
  assert "demo.yml exists" "yes" "no"
fi

echo
echo "acp-pipeline-shape: $PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
