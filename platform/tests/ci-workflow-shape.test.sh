#!/usr/bin/env bash
# #2481: hermetic shape test for .github/workflows/quality.yml.
# Validates that the CI workflow exists, parses, and is wired to run the
# lint-ratchet on PRs to main + pushes to main. The workflow's behavioral
# correctness (clean passes / regression fails) is covered by
# lint-ratchet.test.sh — this test only verifies pipeline plumbing.

set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORKFLOW="$REPO/.github/workflows/quality.yml"

PASSED=0
FAILED=0

assert() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASSED=$((PASSED + 1))
  else
    echo "  FAIL: $label — expected='$expected' actual='$actual'"
    FAILED=$((FAILED + 1))
  fi
}

# Node helper: load yaml via js-yaml from chorus repo node_modules.
yq() {
  node -e "
const y = require('$REPO/node_modules/js-yaml');
const fs = require('fs');
const d = y.load(fs.readFileSync('$WORKFLOW','utf8'));
$1
" 2>/dev/null
}

# 1. Workflow file exists.
if [ -f "$WORKFLOW" ]; then
  assert "workflow file exists" "yes" "yes"
else
  assert "workflow file exists" "yes" "no"
  echo "Aborting — no workflow to inspect."
  exit 1
fi

# 2. Parses as valid YAML.
PARSE_OK=$(yq "process.stdout.write(d ? 'yes' : 'no');" || echo "no")
assert "workflow parses as YAML" "yes" "$PARSE_OK"

# 3. Triggers on push to main. Note: yaml `on:` becomes JS true key when bare,
#    but with subkeys (push/pull_request) the parser keeps it as 'on'.
PUSH_MAIN=$(yq "
const on = d.on || d[true] || {};
const branches = (on.push && on.push.branches) || [];
process.stdout.write(branches.includes('main') ? 'yes' : 'no');
")
assert "triggers on push to main" "yes" "$PUSH_MAIN"

# 4. Triggers on pull_request to main.
PR_MAIN=$(yq "
const on = d.on || d[true] || {};
const pr = on.pull_request || {};
const branches = pr.branches || [];
process.stdout.write(branches.includes('main') ? 'yes' : 'no');
")
assert "triggers on pull_request to main" "yes" "$PR_MAIN"

# 5. lint-ratchet job exists.
HAS_JOB=$(yq "
process.stdout.write((d.jobs && d.jobs['lint-ratchet']) ? 'yes' : 'no');
")
assert "lint-ratchet job exists" "yes" "$HAS_JOB"

# 6. lint-ratchet job runs 'npm run lint:ratchet'.
RUNS_RATCHET=$(yq "
const job = (d.jobs && d.jobs['lint-ratchet']) || {};
const steps = job.steps || [];
const runs = steps.map(s => s.run || '').join(' ');
process.stdout.write(runs.includes('npm run lint:ratchet') ? 'yes' : 'no');
")
assert "job runs 'npm run lint:ratchet'" "yes" "$RUNS_RATCHET"

# 7. Node 20 pinned via actions/setup-node.
NODE_PINNED=$(yq "
const job = (d.jobs && d.jobs['lint-ratchet']) || {};
const steps = job.steps || [];
let pinned = 'no';
for (const s of steps) {
  if (s.uses && s.uses.startsWith('actions/setup-node')) {
    const nv = String((s.with && s.with['node-version']) || '');
    if (nv.startsWith('20')) pinned = 'yes';
    break;
  }
}
process.stdout.write(pinned);
")
assert "Node 20 pinned via setup-node" "yes" "$NODE_PINNED"

echo
echo "ci-workflow-shape: $PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
