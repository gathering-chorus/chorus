#!/usr/bin/env bats
# #2925 AC2 — deploy-daemon-card.sh wrapper.
# One verb that sequences: chorus-werk-sync → chorus-deploy chorus-api →
# probe → cards done. Refuses on missing card ID, non-silas $DEPLOY_ROLE,
# or any step failure. Probe hook for AC4 rollback (rollback impl lands in AC4).

SCRIPT="$BATS_TEST_DIRNAME/../scripts/deploy-daemon-card.sh"

setup() {
  STUBDIR=$(mktemp -d -t deploy-daemon-test.XXXXXX)
  CALLS="$STUBDIR/calls.log"
  : > "$CALLS"

  # Stub PATH commands the wrapper invokes. Each stub records its argv + exits 0.
  for cmd in chorus-werk-sync chorus-deploy cards; do
    cat > "$STUBDIR/$cmd" <<EOF
#!/bin/bash
echo "$cmd \$*" >> "$CALLS"
exit \${STUB_${cmd//-/_}_EXIT:-0}
EOF
    chmod +x "$STUBDIR/$cmd"
  done

  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  # Override probe via flag so the wrapper doesn't have to read a card body in tests.
}

teardown() {
  rm -rf "$STUBDIR"
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
  run "$SCRIPT" 2925 --probe "echo ok"
  [ "$status" -ne 0 ]
  [[ "$output" == *"role"* ]] || [[ "$output" == *"silas"* ]]
}

@test "DEPLOY_ROLE=kade: refuses" {
  export DEPLOY_ROLE="kade"
  run "$SCRIPT" 2925 --probe "echo ok"
  [ "$status" -ne 0 ]
  [[ "$output" == *"silas"* ]]
}

@test "happy path: sequences werk-sync → chorus-deploy → probe → cards done" {
  run "$SCRIPT" 2925 --probe "echo PROBE_OK"
  [ "$status" -eq 0 ]
  # Verify call order
  grep -n 'chorus-werk-sync' "$CALLS"
  grep -n 'chorus-deploy chorus-api' "$CALLS"
  grep -n 'cards done 2925' "$CALLS"
  # Order check
  werk_line=$(grep -n 'chorus-werk-sync' "$CALLS" | cut -d: -f1)
  deploy_line=$(grep -n 'chorus-deploy chorus-api' "$CALLS" | cut -d: -f1)
  done_line=$(grep -n 'cards done 2925' "$CALLS" | cut -d: -f1)
  [ "$werk_line" -lt "$deploy_line" ]
  [ "$deploy_line" -lt "$done_line" ]
}

@test "werk-sync fails: aborts before chorus-deploy" {
  export STUB_chorus_werk_sync_EXIT=1
  run "$SCRIPT" 2925 --probe "echo ok"
  [ "$status" -ne 0 ]
  ! grep -q 'chorus-deploy' "$CALLS"
}

@test "chorus-deploy fails: aborts before probe and cards-done" {
  export STUB_chorus_deploy_EXIT=1
  run "$SCRIPT" 2925 --probe "echo SHOULD_NOT_RUN"
  [ "$status" -ne 0 ]
  ! grep -q 'cards done' "$CALLS"
  ! grep -q 'SHOULD_NOT_RUN' "$output"
}

@test "probe fails: aborts before cards-done (rollback hook point for AC4)" {
  run "$SCRIPT" 2925 --probe "exit 1"
  [ "$status" -ne 0 ]
  ! grep -q 'cards done' "$CALLS"
  [[ "$output" == *"probe"* ]] || [[ "$output" == *"smoke"* ]] || [[ "$output" == *"rollback"* ]]
}

@test "missing probe and no --probe flag: refuses (AC3 will enforce upstream)" {
  run "$SCRIPT" 9999999
  [ "$status" -ne 0 ]
  [[ "$output" == *"probe"* ]]
}
