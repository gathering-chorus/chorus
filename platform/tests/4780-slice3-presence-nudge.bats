#!/usr/bin/env bats
# @test-type: integration — drives chorus-principal with login-harness stub world
# @domain: identity — guards #4786 (Slice 3 of #4780): Presence hostAccount & cleanup
#
# AC3: Presence records real hostAccount, retires on run end, and cleans dangling deliveredTo.
# Negative proof (#3734): a fixture where launcher USER is passed does not contaminate
# hostAccount, and presence after run end is marked unreachable.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }

@test "AC3: login records role account in hostAccount, not launcher USER" {
  export USER="jeffbridwell"
  run "$SCRIPT" on wren
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-wren"'
  test -z "$(printf '%s' "$s" | grep -F '"hostAccount":"jeffbridwell"' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-wren"'
  test -z "$(printf '%s' "$p" | grep -F '"hostAccount":"jeffbridwell"' || true)"
}

@test "NEGATIVE PROOF: launcher USER is rejected from contaminating hostAccount" {
  export USER="attacker-or-launcher"
  run "$SCRIPT" on silas
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$s" | grep -F 'attacker-or-launcher' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$p" | grep -F 'attacker-or-launcher' || true)"
}

@test "AC3: presence retires to unreachable on run end and logs spine event" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren
  test "$status" -eq 0

  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1)
  has "$p" '"reachability":"unreachable"'

  grep -q "session.presence.retired wren" "$T/spine.log"
}

@test "NEGATIVE PROOF: a retired presence does not stay reachable or unknown" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren

  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1)
  test -z "$(printf '%s' "$p" | grep -F '"reachability":"reachable"' || true)"
  test -z "$(printf '%s' "$p" | grep -F '"reachability":"unknown"' || true)"
}

@test "NEGATIVE PROOF: CHORUS_ROLE_ACCOUNT=chorus-kade for silas -> still chorus-silas" {
  export CHORUS_ROLE_ACCOUNT="chorus-kade"
  run "$SCRIPT" on silas
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$s" | grep -F '"hostAccount":"chorus-kade"' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$p" | grep -F '"hostAccount":"chorus-kade"' || true)"
}

@test "NEGATIVE PROOF: principal with missing hostAccount in identity door is refused loudly" {
  printf '{"data":{"principalKind":"agent"}}\n200\n' > "$T/principal-silas.json"
  run "$SCRIPT" on silas
  test "$status" -ne 0
  out_has "REFUSED"
  out_has "principal silas has no hostAccount bound in the identity door"
}


# --- S2 (AC1 / AC2): Nudge-health judges delivery results and run liveness ---

@test "AC1/AC2: nudge-health judges run liveness and Pulse agreement without probing Terminal windows" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-s2"
  mkdir -p "$W/sessions" "$W/canary"

  # Register live sessions for the roles
  for r in wren silas kade borg-queen abby-normal; do
    printf '{"role":"%s","pid":%s,"tty":"/dev/ttys999","host":"unknown","registered_at":"9999999999"}' \
      "$r" "$$" > "$W/sessions/$r-$$.json"
  done

  # Stub Pulse returning live actors
  PULSE_JSON='{"actors":['
  first=1
  for r in wren silas kade borg-queen abby-normal; do
    [ "$first" -eq 1 ] && first=0 || PULSE_JSON="${PULSE_JSON},"
    PULSE_JSON="${PULSE_JSON}{\"aboutPrincipal\":\"principal-${r}\",\"sessions\":[{\"live\":true}]}"
  done
  PULSE_JSON="${PULSE_JSON}]}"

  run env CHORUS_ROOT="$ROOT" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          pulse_actors="$PULSE_JSON" \
          MESSAGES_DB="/dev/null" \
          bash "$HEALTH"
  test "$status" -eq 0
  out_has "NUDGE HEALTH: all roles reachable"
  out_lacks "Terminal"
  out_lacks "no-window"
}

@test "AC1/AC2: a live role on its own account stays green in nudge-health" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-s2-single"
  mkdir -p "$W/sessions" "$W/canary"

  printf '{"role":"silas","pid":%s,"tty":"/dev/ttys999","host":"unknown"}' \
    "$$" > "$W/sessions/silas-$$.json"

  run env CHORUS_ROOT="$ROOT" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          ROLES_OVERRIDE="silas" \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-silas","sessions":[{"live":true}]}]}' \
          MESSAGES_DB="/dev/null" \
          bash "$HEALTH"
  test "$status" -eq 0
  out_has "OK: silas — live session on host=unknown (pid $$), agreed with Pulse (pulse_online=true)"
  out_has "NUDGE HEALTH: all roles reachable"
}

@test "NEGATIVE PROOF (#3734): a refused nudge turns nudge-health RED and names the role" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-s2-fail"
  mkdir -p "$W/sessions" "$W/canary"

  # Case A: delivery error recorded in messages.db
  run env CHORUS_ROOT="$ROOT" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          ROLES_OVERRIDE="borg-queen" \
          STUB_FAILED_NUDGES="borg-queen|unknown role: borg-queen" \
          bash "$HEALTH"
  test "$status" -ne 0
  out_has "FAIL: borg-queen — nudge delivery failed: unknown role: borg-queen"
  out_has "NUDGE HEALTH: 1 role(s) have issues"

  # Case B: delivery path refuses unknown actor
  run env CHORUS_ROOT="$ROOT" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          ROLES_OVERRIDE="unregistered-intruder" \
          STUB_FAILED_NUDGES="" \
          bash "$HEALTH"
  test "$status" -ne 0
  out_has "FAIL: unregistered-intruder — delivery path refuses role (unknown role: unregistered-intruder)"
  out_has "NUDGE HEALTH: 1 role(s) have issues"
}


@test "NEGATIVE PROOF (#3734): unreadable roles door is UNMEASURED red, never silent fallback" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-s2-door-down"
  mkdir -p "$W/sessions" "$W/canary"

  run env CHORUS_ROOT="$ROOT" \
          CHORUS_ROLES_URL="http://127.0.0.1:9/unreachable-roles-door" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          bash "$HEALTH"
  test "$status" -eq 2
  out_has "UNMEASURED — roles door is unreadable"
}

@test "NEGATIVE PROOF (#3734): unreadable Pulse actor-state is UNMEASURED red, never silent pass" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-s2-pulse-down"
  mkdir -p "$W/sessions" "$W/canary"

  # Pulse URL unreadable / failing -> must exit 2 UNMEASURED
  run env CHORUS_ROOT="$ROOT" \
          PULSE_URL="http://127.0.0.1:9/unreachable-pulse" \
          CHORUS_SESSIONS_DIR="$W/sessions" \
          CANARY_DIR="$W/canary" \
          bash "$HEALTH"
  test "$status" -eq 2
  out_has "UNMEASURED — Pulse actor-state is unreadable"
}

# --- S3 (AC4): Bouncer writes to $CHORUS_STATE_DIR/pending-approvals/<role>/, responder reads only that ---

@test "AC4: login provisions \$CHORUS_STATE_DIR/pending-approvals/<role> with 0750 permissions" {
  export USER="jeffbridwell"
  run "$SCRIPT" on wren
  test "$status" -eq 0

  grep -q "mkdir -p .*pending-approvals/wren" "$T/sudo.log"
  grep -q "chmod 0750 .*pending-approvals/wren" "$T/sudo.log"
}

@test "AC4: role_pending_dir resolves to CHORUS_STATE_DIR/pending-approvals/<role>" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks role_pending_dir_resolves_to_state_dir_pending_approvals_role
  test "$status" -eq 0
  out_has "test hooks::card_approval_responder::tests::role_pending_dir_resolves_to_state_dir_pending_approvals_role ... ok"
}

@test "AC4 NEGATIVE PROOF (#3734): a file under /Users/chorus-<role>/ is NOT picked up" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks negative_proof_file_under_role_home_is_not_picked_up
  test "$status" -eq 0
  out_has "test hooks::card_approval_responder::tests::negative_proof_file_under_role_home_is_not_picked_up ... ok"
}
