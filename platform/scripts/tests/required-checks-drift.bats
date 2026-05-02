#!/usr/bin/env bats

# #2500 — required-checks drift detector contract tests.
#
# Source of truth: platform/state/required-checks.json
# Drift surfaces:
#   1. quality.yml contains the canonical jobs?
#   2. GitHub branch-protection required_status_checks matches?
#   3. GitHub rulesets required_status_checks matches?
#
# Drift = mismatch in any of (1), (2), (3) vs the canonical list.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../required-checks-drift.sh"
  [ -f "$SCRIPT" ] || skip "required-checks-drift.sh not yet present"

  CANONICAL="$BATS_TEST_TMPDIR/required-checks.json"
  cat > "$CANONICAL" <<'JSON'
{
  "schema_version": 1,
  "_doc": "Canonical list of required CI checks per DEC-2525.",
  "required": [
    {"name": "unit-tests", "card": 2526, "added": "2026-04-25"},
    {"name": "cargo-test", "card": 2526, "added": "2026-04-25"}
  ]
}
JSON

  WORKFLOW="$BATS_TEST_TMPDIR/quality.yml"
  cat > "$WORKFLOW" <<'YAML'
name: quality
on: [push, pull_request]
jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps: [{run: "echo unit"}]
  cargo-test:
    runs-on: ubuntu-latest
    steps: [{run: "echo cargo"}]
  lint-ratchet:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps: [{run: "echo lint"}]
YAML
}

@test "list-canonical: prints the required check names from canonical JSON" {
  REQUIRED_CHECKS_FILE="$CANONICAL" run bash "$SCRIPT" list-canonical
  [ "$status" -eq 0 ]
  [[ "$output" == *"unit-tests"* ]]
  [[ "$output" == *"cargo-test"* ]]
}

@test "list-workflow: extracts top-level job names from a workflow YAML" {
  run bash "$SCRIPT" list-workflow "$WORKFLOW"
  [ "$status" -eq 0 ]
  [[ "$output" == *"unit-tests"* ]]
  [[ "$output" == *"cargo-test"* ]]
  [[ "$output" == *"lint-ratchet"* ]]
}

@test "diff-workflow: passes when canonical jobs are all in workflow" {
  REQUIRED_CHECKS_FILE="$CANONICAL" run bash "$SCRIPT" diff-workflow "$WORKFLOW"
  [ "$status" -eq 0 ]
}

@test "diff-workflow: fails when a canonical job is missing from workflow" {
  cat > "$BATS_TEST_TMPDIR/missing.yml" <<'YAML'
name: quality
jobs:
  cargo-test:
    runs-on: ubuntu-latest
YAML
  REQUIRED_CHECKS_FILE="$CANONICAL" run bash "$SCRIPT" diff-workflow "$BATS_TEST_TMPDIR/missing.yml"
  [ "$status" -ne 0 ]
  [[ "$output" == *"unit-tests"* ]]
  [[ "$output" == *"missing"* ]] || [[ "$output" == *"DRIFT"* ]]
}

@test "missing canonical file: nothing is canonical (fail-closed)" {
  REQUIRED_CHECKS_FILE="/tmp/nonexistent-required-checks.json" run bash "$SCRIPT" list-canonical
  [ "$status" -ne 0 ]
}

@test "help when invoked with no args" {
  run bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" == *"Usage"* ]] || [[ "$output" == *"usage"* ]]
}
