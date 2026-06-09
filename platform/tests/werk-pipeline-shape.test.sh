#!/usr/bin/env bash
# #3236: hermetic shape test for werk.yml — THE pipeline, one act-orchestrated,
# MCP-toolchain workflow. Decided 2026-06-04 (Jeff): one orchestrator (act), one
# toolchain (MCP), ONE file — collapse the two-orchestrator drift (bash werk-mcp.sh +
# direct-binary acp.yml) AND don't re-split into demo.yml/acp.yml (that just
# re-introduces the multiplicity werk-mcp.sh already collapsed). werk.yml mimics
# werk-mcp.sh 1:1: prove in the slot, then land, stopping before accept.
#
# Shape/plumbing only — that werk.yml exists and is wired to the invariants below.
# Live act-run behavior is integration-gated (chorus-api + real verbs), covered
# separately. Dependency-free (grep, no YAML lib): a shape test must not need a parser.
#
# Invariants (each maps to a card AC + an upstream fix the act path must carry):
#   - workflow_dispatch trigger
#   - full prove→land sequence: commit, push, build, deploy(werk), env-up, demo, merge,
#     sync-canonical, deploy(canonical)
#   - does NOT invoke werk-accept; DOES print the accept command  (#3234 stop-before-accept, DEC-048)
#   - canonical ff-sync present, live-claim gated on SYNC_OK       (#3234 merged≠live)
#   - NO standalone `git rebase` step                             (#3186 rebase is inside werk-commit)
#   - verbs go through the MCP helper; no bare verb-binary call    (single toolchain)
#   - exactly ONE pipeline workflow (no demo.yml/acp.yml split)

set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WF="$REPO/.github/workflows"
WERK="$WF/werk.yml"

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

present() { grep -Eq "$2" "$1" 2>/dev/null && echo yes || echo no; }
absent()  { grep -Eq "$2" "$1" 2>/dev/null && echo no  || echo yes; }

# ---------------------------------------------------------------- werk.yml exists
if [ -f "$WERK" ]; then
  assert "werk.yml exists" "yes" "yes"
else
  assert "werk.yml exists" "yes" "no"; echo "Aborting."; exit 1
fi

# ONE pipeline workflow: the old split files must be gone.
assert "no demo.yml split (collapsed into werk.yml)" "yes" \
  "$([ -f "$WF/demo.yml" ] && echo no || echo yes)"
assert "no acp.yml split (collapsed into werk.yml)" "yes" \
  "$([ -f "$WF/acp.yml" ] && echo no || echo yes)"

assert "werk.yml triggers on workflow_dispatch" "yes" \
  "$(present "$WERK" 'workflow_dispatch')"

# Full prove→land sequence present.
for tool in werk-commit werk-push werk-build chorus_env_up werk-merge; do
  assert "werk.yml runs $tool (via MCP)" "yes" "$(present "$WERK" "$tool")"
done
assert "werk.yml deploys to werk slot then canonical" "yes" \
  "$(present "$WERK" 'target.{0,4}werk')"
assert "werk.yml deploys to canonical (land)" "yes" \
  "$(present "$WERK" 'target.{0,9}canonical')"
# Test is its OWN step before demo (Jeff's model: test AND demo at end).
assert "werk.yml has an explicit test step (cargo/jest hermetic gate)" "yes" \
  "$(present "$WERK" 'cargo test|npx jest')"
assert "werk.yml runs the demo (werk-demo)" "yes" \
  "$(present "$WERK" 'werk-demo')"

# #3234: stop-before-accept — no real werk-accept invocation, but DOES print the command.
assert "werk.yml does NOT invoke werk-accept (#3234)" "yes" \
  "$(absent "$WERK" 'chorus-mcp-call\.sh[[:space:]]+[a-z]+[[:space:]]+werk-accept|^[[:space:]]*werk-accept[[:space:]]')"
assert "werk.yml prints the accept command for the human (#3234/DEC-048)" "yes" \
  "$(present "$WERK" 'echo.*werk-accept')"

# #3234: canonical ff-sync + SYNC_OK-gated live-claim.
assert "werk.yml has canonical ff-sync step (#3234)" "yes" \
  "$(present "$WERK" 'merge[[:space:]]+--ff-only[[:space:]]+origin/main')"
assert "werk.yml gates live-claim on sync (SYNC_OK #3234)" "yes" \
  "$(present "$WERK" 'SYNC_OK')"

# #3186: no standalone rebase step.
assert "werk.yml has NO standalone git rebase step (#3186)" "yes" \
  "$(absent "$WERK" 'git[[:space:]].*rebase[[:space:]]+origin/main')"

# Single toolchain: MCP helper used; no bare verb-binary invocation.
assert "werk.yml calls verbs via MCP helper (single toolchain)" "yes" \
  "$(present "$WERK" 'chorus-mcp-call|tools/call')"
assert "werk.yml has no bare verb-binary invocation" "yes" \
  "$(absent "$WERK" '^[[:space:]]*(werk-commit|werk-push|werk-merge|werk-build|werk-deploy)[[:space:]]')"

echo
echo "werk-pipeline-shape: $PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
