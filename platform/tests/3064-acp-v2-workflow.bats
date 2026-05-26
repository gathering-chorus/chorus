#!/usr/bin/env bats
# Integration tests for the v2 /acp-v2 workflow (#3064).
#
# Mirrors the verb-binary e2e pattern: real `act` invocation + PATH-shimmed verb
# binaries + temp CHORUS_HOME / CHORUS_WERK_BASE. The shims record DEPLOY_ROLE,
# CARD_ID, ROLE, CHORUS_TRACE_ID — so we can assert on env propagation and
# step ordering without actually exercising the real verbs against the real repo.
# Each test isolates env in BATS_TEST_TMPDIR.
#
# Covers AC1 (act sequences), AC2 (shared trace propagates), AC3 (stop on
# failure), AC4/AC5 (orchestrator jsonl + per-step DEPLOY_ROLE rebind).

WORKFLOW="${BATS_TEST_DIRNAME}/../../.github/workflows/acp.yml"

setup() {
  TEST_ROOT="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$TEST_ROOT/home"
  export CHORUS_WERK_BASE="$TEST_ROOT/werk-base"
  mkdir -p "$CHORUS_HOME/ops/logs" "$CHORUS_WERK_BASE" "$TEST_ROOT/bin" "$TEST_ROOT/origin"
  # PATH: shim verbs first; keep system git/act/gh for the real bits.
  export PATH="$TEST_ROOT/bin:$PATH"
  export SHIM_LOG="$TEST_ROOT/shim.log"
  : > "$SHIM_LOG"

  # One shared origin per test (rebase step needs it).
  local origin="$TEST_ROOT/origin"
  git -C "$origin" init -q -b main
  git -C "$origin" config user.email t@t
  git -C "$origin" config user.name t
  echo init > "$origin/README"
  git -C "$origin" add README
  git -C "$origin" commit -q -m init
  git -C "$origin" config receive.denyCurrentBranch ignore
}

# Write a shim that logs invocation + key env, exits with the given code (default 0).
write_shim() {
  local name="$1" exit_code="${2:-0}"
  cat > "$TEST_ROOT/bin/$name" <<EOF
#!/bin/sh
echo "[$name] DEPLOY_ROLE=\$DEPLOY_ROLE CARD_ID=\$CARD_ID ROLE=\$ROLE TRACE=\$CHORUS_TRACE_ID" >> "$SHIM_LOG"
exit $exit_code
EOF
  chmod +x "$TEST_ROOT/bin/$name"
}

write_all_shims_ok() {
  for v in werk-commit werk-push werk-build werk-deploy werk-verify werk-accept; do
    write_shim "$v"
  done
}

# Clone the temp origin into a werk on <role>/<card> branch.
setup_werk() {
  local role="$1" card="$2"
  local werk="$CHORUS_WERK_BASE/${role}-${card}"
  git clone -q "$TEST_ROOT/origin" "$werk"
  git -C "$werk" config user.email t@t
  git -C "$werk" config user.name t
  git -C "$werk" checkout -q -b "${role}/${card}"
}

run_acp_workflow() {
  local card="$1" role="$2" accepter="$3"
  act -P macos-latest=-self-hosted \
      -W "$WORKFLOW" -j acp \
      --input card_id="$card" --input role="$role" --input accepter="$accepter"
}

@test "happy path: all 6 verbs invoked, orchestrator jsonl emits start+completed" {
  write_all_shims_ok
  setup_werk kade 9001

  run run_acp_workflow 9001 kade jeff
  [ "$status" -eq 0 ]

  # Each verb invoked at least once
  grep -q "^\[werk-commit\]" "$SHIM_LOG"
  grep -q "^\[werk-push\]" "$SHIM_LOG"
  grep -q "^\[werk-build\]" "$SHIM_LOG"
  grep -q "^\[werk-deploy\]" "$SHIM_LOG"
  grep -q "^\[werk-accept\]" "$SHIM_LOG"

  # Orchestrator jsonl witness: start + completed paired
  grep -q '"event":"acp.started"' "$CHORUS_HOME/ops/logs/werk-acp.jsonl"
  grep -q '"event":"acp.completed"' "$CHORUS_HOME/ops/logs/werk-acp.jsonl"
}

@test "stop-on-failure: build exits non-zero halts the chain (no deploy/verify/accept)" {
  write_shim werk-commit
  write_shim werk-push
  write_shim werk-build 1     # build fails
  write_shim werk-deploy
  write_shim werk-verify
  write_shim werk-accept
  setup_werk kade 9002

  run run_acp_workflow 9002 kade jeff
  [ "$status" -ne 0 ]

  grep -q "^\[werk-commit\]" "$SHIM_LOG"
  grep -q "^\[werk-build\]"  "$SHIM_LOG"
  ! grep -q "^\[werk-deploy\]" "$SHIM_LOG"
  ! grep -q "^\[werk-verify\]" "$SHIM_LOG"
  ! grep -q "^\[werk-accept\]" "$SHIM_LOG"

  grep -q '"event":"acp.failed"' "$CHORUS_HOME/ops/logs/werk-acp.jsonl"
}

@test "DEPLOY_ROLE rebind: accept sees accepter; other verbs see the builder role" {
  write_all_shims_ok
  setup_werk kade 9003

  run run_acp_workflow 9003 kade jeff
  [ "$status" -eq 0 ]

  # accept step's DEPLOY_ROLE is the accepter (jeff) — the rebind that the pre-push
  # branch-check bug (DEPLOY_ROLE=accepter would refuse a kade/9003 push) forced.
  grep "^\[werk-accept\]" "$SHIM_LOG" | grep -q "DEPLOY_ROLE=jeff"

  # commit step's DEPLOY_ROLE is the builder (kade) — required by the pre-push branch
  # prefix check (#2580/#2598).
  grep "^\[werk-commit\]" "$SHIM_LOG" | grep -q "DEPLOY_ROLE=kade"
  grep "^\[werk-push\]"   "$SHIM_LOG" | grep -q "DEPLOY_ROLE=kade"
  grep "^\[werk-build\]"  "$SHIM_LOG" | grep -q "DEPLOY_ROLE=kade"
}

@test "shared trace: every verb subprocess sees the same CHORUS_TRACE_ID" {
  write_all_shims_ok
  setup_werk kade 9004

  run run_acp_workflow 9004 kade jeff
  [ "$status" -eq 0 ]

  # Exactly one distinct trace across all verb invocations.
  local n_traces; n_traces=$(grep -oE 'TRACE=[^ ]+' "$SHIM_LOG" | sort -u | wc -l | tr -d ' ')
  [ "$n_traces" = "1" ]

  # And it matches the value the orchestrator jsonl recorded at acp.started.
  local jsonl_trace; jsonl_trace=$(grep '"event":"acp.started"' "$CHORUS_HOME/ops/logs/werk-acp.jsonl" \
    | head -1 | grep -oE '"trace_id":"[^"]+"' | sed 's/.*:"//;s/"//')
  local shim_trace; shim_trace=$(grep -oE 'TRACE=[^ ]+' "$SHIM_LOG" | head -1 | sed 's/TRACE=//')
  [ "$jsonl_trace" = "$shim_trace" ]
}
