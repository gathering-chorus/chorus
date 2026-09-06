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

# #4111 — resolve js-yaml, and REFUSE rather than report if it is not there.
#
# This helper used to require it from exactly $REPO/node_modules and swallow
# every error with 2>/dev/null. js-yaml has never been installed at that path —
# not in canonical, not in any werk — so every assertion below returned the
# empty string and the suite reported the CI workflow as malformed. It could not
# tell "the workflow is broken" from "I cannot read YAML", which are the two
# states it exists to separate, and it has been red for the second reason for as
# long as it has existed.
#
# js-yaml IS installed under the package node_modules (platform/api and others).
# Resolve from the first that has it; if none does, say so and exit UNMEASURED
# rather than blaming the workflow.
JS_YAML=""
for cand in "$REPO/node_modules/js-yaml" \
            "$REPO/platform/api/node_modules/js-yaml" \
            "$REPO/platform/pulse/node_modules/js-yaml" \
            "$REPO/platform/chorus-sdk/node_modules/js-yaml"; do
  if [ -d "$cand" ]; then JS_YAML="$cand"; break; fi
done
if [ -z "$JS_YAML" ]; then
  echo "ci-workflow-shape: UNMEASURED — js-yaml not resolvable under $REPO;"
  echo "  the CI workflow was NOT inspected. This is a missing parser, not a"
  echo "  malformed workflow. Run npm install in platform/api (or the root)."
  echo
  echo "ci-workflow-shape: 0 passed, 0 failed (UNMEASURED — js-yaml missing)"
  exit 0
fi

# Node helper: load yaml via the resolved js-yaml. stderr is deliberately NOT
# swallowed — a parser that dies must say why, not return an empty string that
# reads as a failed assertion.
yq() {
  node -e "
const y = require('$JS_YAML');
const fs = require('fs');
const d = y.load(fs.readFileSync('$WORKFLOW','utf8'));
$1
"
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

# 3+4. The per-branch triggers are ABSENT, on purpose.
#
# #4111 — these two assertions used to demand `push:` and `pull_request:` on
# main. That lane was cost-killed on 2026-04-29 (#2526 wave 5, ADR-026 layer 3
# demoted to schedule-only; GHA spend was $225/mo projected and Jeff named it
# unsustainable). quality.yml has said so in a comment block at its `on:` key
# ever since. The guard kept asserting the killed lane for four months and
# nobody saw it, because the whole suite was already failing on a missing
# parser — one broken check hid another.
#
# So encode the DECISION, not the pre-decision shape. If someone re-enables the
# triggers, this goes red and they have to say why the cost stop is over — which
# is the conversation worth having. Re-enable path is documented in quality.yml.
# Note: yaml `on:` becomes the JS true key when bare, but with subkeys the
# parser keeps it as 'on'.
PUSH_MAIN=$(yq "
const on = d.on || d[true] || {};
const branches = (on.push && on.push.branches) || [];
process.stdout.write(branches.includes('main') ? 'yes' : 'no');
")
assert "no push-to-main trigger (cost stop 2026-04-29, #2526 wave 5)" "no" "$PUSH_MAIN"

PR_MAIN=$(yq "
const on = d.on || d[true] || {};
const pr = on.pull_request || {};
const branches = pr.branches || [];
process.stdout.write(branches.includes('main') ? 'yes' : 'no');
")
assert "no pull_request-to-main trigger (same cost stop)" "no" "$PR_MAIN"

# The workflow must still be reachable by hand — the cost stop removed the
# automatic triggers, it did not retire the workflow.
HAS_DISPATCH=$(yq "
const on = d.on || d[true] || {};
process.stdout.write(('workflow_dispatch' in on) ? 'yes' : 'no');
")
assert "still runnable on demand (workflow_dispatch)" "yes" "$HAS_DISPATCH"

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
