#!/usr/bin/env bats
# deploy-daemon-card.sh wrapper — #2925 (original) reworked by #2927.
#
# #2927 contract: one verb sequences chorus-werk-sync → per-unit deploy_<unit>
# (chorus-api / chorus-hooks / cards-sdk) → probe → cards done. Units resolve
# from --units or diff-introspect against origin/main in the card's werk.
# Per-unit deploy authority (#2927 AC4): any of kade/wren/silas may invoke;
# DEC-022 LaunchAgent/cdhash narrowing follows the unit's domain owner, not a
# hardcoded silas check. A probe is mandatory; probe failure rolls back.
#
# The deploy/rollback steps are internal functions (deploy_chorus_api etc.)
# that do real npm-build + rsync + kickstart, so the testable seam is to
# `source` the script (its source-guard skips main) and override those
# functions + a fake werk, rather than stub a `chorus-deploy` CLI (the
# pre-#2927 seam, now gone).

SCRIPT="$BATS_TEST_DIRNAME/../scripts/deploy-daemon-card.sh"

setup() {
  STUBDIR=$(mktemp -d -t deploy-daemon-test.XXXXXX)
  CALLS="$STUBDIR/calls.log"
  : > "$CALLS"

  # PATH stubs for the external commands main() shells out to.
  for cmd in chorus-werk-sync cards chorus-log; do
    cat > "$STUBDIR/$cmd" <<EOF
#!/bin/bash
echo "$cmd \$*" >> "$CALLS"
exit \${STUB_${cmd//-/_}_EXIT:-0}
EOF
    chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"

  # Fake werk for card 2925 with a platform/api surface so unit resolution +
  # deploy_chorus_api's werk check are satisfied.
  export CHORUS_WERK_BASE="$STUBDIR/werk"
  mkdir -p "$CHORUS_WERK_BASE/silas-2925/platform/api"

  export DEPLOY_ROLE="silas"
}

teardown() {
  rm -rf "$STUBDIR"
}

# Source the script (source-guard means main is NOT auto-run), then override
# the internal per-unit functions to record calls instead of building/rsyncing.
load_with_stub_units() {
  source "$SCRIPT"
  # The script sets `set -euo pipefail`; sourcing leaks that into the test body
  # where grep -q misses and `[ ]` falses are normal control flow, not errors.
  set +euo pipefail
  # Per-unit exit control: STUB_DEPLOY_<UNIT>_EXIT (default 0). Rollbacks record.
  deploy_chorus_api()     { echo "deploy chorus-api $*"     >> "$CALLS"; return "${STUB_DEPLOY_CHORUS_API_EXIT:-0}"; }
  deploy_chorus_hooks()   { echo "deploy chorus-hooks $*"   >> "$CALLS"; return "${STUB_DEPLOY_CHORUS_HOOKS_EXIT:-0}"; }
  rollback_chorus_api()   { echo "rollback chorus-api $*"   >> "$CALLS"; return 0; }
  rollback_chorus_hooks() { echo "rollback chorus-hooks $*" >> "$CALLS"; return 0; }
}

@test "script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "no args: exits non-zero with usage" {
  run "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" == *"usage"* ]] || [[ "$output" == *"Usage"* ]]
}

@test "non-numeric card id: refuses" {
  run "$SCRIPT" "not-a-number"
  [ "$status" -ne 0 ]
}

@test "DEPLOY_ROLE unset: refuses with role message" {
  unset DEPLOY_ROLE
  run "$SCRIPT" 2925 --probe "echo ok" --units chorus-api
  [ "$status" -ne 0 ]
  [[ "$output" == *"role"* ]] || [[ "$output" == *"DEPLOY_ROLE"* ]]
}

@test "unknown DEPLOY_ROLE: refuses naming the valid roles (#2927 per-unit, not silas-only)" {
  export DEPLOY_ROLE="bogus"
  run "$SCRIPT" 2925 --probe "echo ok" --units chorus-api
  [ "$status" -ne 0 ]
  [[ "$output" == *"kade"* ]] && [[ "$output" == *"wren"* ]] && [[ "$output" == *"silas"* ]]
}

@test "happy path: sequences werk-sync → deploy unit → probe → cards done" {
  load_with_stub_units
  run main 2925 --probe "echo PROBE_OK" --units chorus-api
  [ "$status" -eq 0 ]
  grep -q 'werk-sync' "$CALLS"
  grep -q 'deploy chorus-api' "$CALLS"
  grep -q 'cards done 2925' "$CALLS"
  # Order: werk-sync before deploy before cards-done.
  werk_line=$(grep -n 'werk-sync' "$CALLS" | head -1 | cut -d: -f1)
  deploy_line=$(grep -n 'deploy chorus-api' "$CALLS" | head -1 | cut -d: -f1)
  done_line=$(grep -n 'cards done 2925' "$CALLS" | head -1 | cut -d: -f1)
  [ "$werk_line" -lt "$deploy_line" ]
  [ "$deploy_line" -lt "$done_line" ]
}

@test "werk-sync fails: aborts before deploy" {
  export STUB_chorus_werk_sync_EXIT=1
  load_with_stub_units
  run main 2925 --probe "echo ok" --units chorus-api
  [ "$status" -ne 0 ]
  ! grep -q 'deploy chorus-api' "$CALLS"
}

@test "deploy fails: aborts before probe and cards-done" {
  # Single-unit deploy failure: nothing succeeded yet, so there's nothing to
  # roll back — the contract is just abort-before-cards-done. Cross-unit
  # rollback is exercised by the multi-unit tests below.
  export STUB_DEPLOY_CHORUS_API_EXIT=1
  load_with_stub_units
  run main 2925 --probe "echo SHOULD_NOT_RUN" --units chorus-api
  [ "$status" -ne 0 ]
  ! grep -q 'cards done' "$CALLS"
  ! grep -q 'SHOULD_NOT_RUN' "$CALLS"
}

@test "multi-unit: a later unit's deploy-fail rolls back the earlier succeeded unit" {
  # Units resolve sorted: chorus-api then chorus-hooks. chorus-api deploys OK
  # (succeeded), chorus-hooks deploy FAILS → the wrapper must roll back the
  # already-succeeded chorus-api and abort before cards-done (script:314-333).
  export STUB_DEPLOY_CHORUS_HOOKS_EXIT=1
  load_with_stub_units
  run main 2925 --probe "echo ok" --units "chorus-api,chorus-hooks"
  [ "$status" -ne 0 ]
  grep -q 'deploy chorus-api' "$CALLS"
  grep -q 'rollback chorus-api' "$CALLS"   # earlier success rolled back
  ! grep -q 'rollback chorus-hooks' "$CALLS"  # failed unit never "succeeded"
  ! grep -q 'cards done' "$CALLS"
}

@test "multi-unit probe-fail: rolls back succeeded units in REVERSE order" {
  # Both units deploy OK, then probe fails → roll back all succeeded units in
  # reverse of deploy order: chorus-hooks BEFORE chorus-api (script:328 reverse_lines).
  load_with_stub_units
  run main 2925 --probe "exit 1" --units "chorus-api,chorus-hooks"
  [ "$status" -ne 0 ]
  ! grep -q 'cards done' "$CALLS"
  grep -q 'rollback chorus-api' "$CALLS"
  grep -q 'rollback chorus-hooks' "$CALLS"
  hooks_line=$(grep -n 'rollback chorus-hooks' "$CALLS" | head -1 | cut -d: -f1)
  api_line=$(grep -n 'rollback chorus-api' "$CALLS" | head -1 | cut -d: -f1)
  [ "$hooks_line" -lt "$api_line" ]   # reverse of deploy order
}

@test "probe fails: rolls back, aborts before cards-done" {
  load_with_stub_units
  run main 2925 --probe "exit 1" --units chorus-api
  [ "$status" -ne 0 ]
  ! grep -q 'cards done' "$CALLS"
  grep -q 'rollback chorus-api' "$CALLS"
  [[ "$output" == *"probe"* ]] || [[ "$output" == *"rollback"* ]]
}

@test "missing --probe: refuses (probe is mandatory)" {
  run "$SCRIPT" 2925 --units chorus-api
  [ "$status" -ne 0 ]
  [[ "$output" == *"probe"* ]]
}

@test "zero units matched: refuses (AC5)" {
  # Empty werk diff + no --units → no known unit paths → zero-match refusal.
  rm -rf "$CHORUS_WERK_BASE/silas-2925/platform/api"
  run "$SCRIPT" 2925 --probe "echo ok"
  [ "$status" -ne 0 ]
}
