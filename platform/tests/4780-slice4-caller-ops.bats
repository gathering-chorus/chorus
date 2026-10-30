#!/usr/bin/env bats
# @test-type: integration
# @domain: identity — guards #4787 (Slice 4 of #4780): verified caller identity, narrow ops helper, named service events

setup_file() {
  # Build target shared across tests in this suite, hermetic to runner account
  export CARGO_TARGET_DIR="$BATS_FILE_TMPDIR/cargo-target"
}

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$ROOT"
  export CHORUS_LOG_FILE="$T/chorus.log"
  export LOKI_FIRST_OVERRIDE=1
  export CARGO_TARGET_DIR="$BATS_FILE_TMPDIR/cargo-target"
}

# --- AC3: service.started and service.stopped spine events name the service ---

@test "AC3: service.started and service.stopped spine events name the service" {
  LABEL="com.chorus.test-probe"
  bash "$ROOT/platform/scripts/service-run" "$LABEL" job /usr/bin/true

  # Assert service.started exists and carries "service":"$LABEL"
  start_line="$(grep '"event":"service.started"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$start_line"
  [[ "$start_line" =~ \"service\":\"$LABEL\" ]] || return 1

  # Assert service.stopped exists and carries "service":"$LABEL"
  stop_line="$(grep '"event":"service.stopped"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$stop_line"
  [[ "$stop_line" =~ \"service\":\"$LABEL\" ]] || return 1
}

@test "AC3 NEGATIVE PROOF (#3734): service-run emitting without service field fails verification" {
  LABEL="com.chorus.test-probe-neg"
  _SERVICE_RUN_TEST_OMIT_SERVICE=1 bash "$ROOT/platform/scripts/service-run" "$LABEL" job /usr/bin/true

  # Verification assertion: event must contain "service":"$LABEL"
  # Under the faulty emission, the assertion fails
  start_line="$(grep '"event":"service.started"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$start_line"
  ! [[ "$start_line" =~ \"service\":\"$LABEL\" ]] || return 1

  stop_line="$(grep '"event":"service.stopped"' "$CHORUS_LOG_FILE" | tail -1)"
  test -n "$stop_line"
  ! [[ "$stop_line" =~ \"service\":\"$LABEL\" ]] || return 1
}

# --- AC1: Caller's role comes from verified account or signed credential, never DEPLOY_ROLE env var or cwd guess ---

@test "AC1: Rust role resolver ignores DEPLOY_ROLE when running as chorus-<role>" {
  cd "$ROOT/platform/services/chorus-hooks"
  cargo test --offline --frozen --lib shared::role::session_resolver_tests_4202::account_chorus_role_ignores_deploy_role_negative_proof
}

@test "AC1: cards detectRole uses OS account chorus-<role> and ignores DEPLOY_ROLE" {
  npm --prefix "$ROOT/directing/products/cards" test tests/config-3600.test.ts
}

@test "AC1 NEGATIVE PROOF (#3734): DEPLOY_ROLE=jeff in a chorus-silas shell does not make the caller jeff" {
  run node -e '
    const { detectRole } = require("'"$ROOT"'/directing/products/cards/dist/config");
    process.env.DEPLOY_ROLE = "jeff";
    const role = detectRole("chorus-silas");
    if (role === "jeff") {
      console.error("FAIL: spoofed role to jeff");
      process.exit(1);
    }
    if (role !== "silas") {
      console.error("FAIL: expected silas, got " + role);
      process.exit(1);
    }
    console.log("PASS: role is " + role);
  '
  test "$status" -eq 0
  [[ "$output" =~ "PASS: role is silas" ]] || return 1
}

@test "AC1 NEGATIVE PROOF (#3734): chorus-<unknown> account with DEPLOY_ROLE=jeff fails loudly and cannot claim jeff" {
  run node -e '
    const { detectRole } = require("'"$ROOT"'/directing/products/cards/dist/config");
    process.env.DEPLOY_ROLE = "jeff";
    try {
      const role = detectRole("chorus-foo");
      console.error("FAIL: unexpectedly succeeded with " + role);
      process.exit(1);
    } catch (err) {
      console.log("PASS: threw " + err.message);
      process.exit(0);
    }
  '
  test "$status" -eq 0
  [[ "$output" =~ "PASS: threw" ]] || return 1
}

# --- AC2: Role on its own account can kickstart, load, and retire owned com.chorus.* LaunchAgents via narrow ops helper ---

@test "AC2: chorus-hooks daemon socket launchagent_ops enforces graph ownedBy" {
  cd "$ROOT/platform/services/chorus-hooks"
  cargo test --offline --frozen --bin chorus-hooks launchagent_ops_tests::negative_proof_launchagent_ops_refuses_unowned_or_mismatched_role
}

@test "AC2: agent socket peer check refuses spoofed deploy_role over socket" {
  cd "$ROOT/platform/services/chorus-hooks"
  cargo test --offline --frozen --lib agent_socket::tests::negative_proof_peer_role_refuses_mismatched_deploy_role
}

@test "AC2 NEGATIVE PROOF (#3734): invalid verbs or arbitrary extra args/paths are refused by launchagent-ops" {
  # Build shim if needed
  SHIM_BIN="${CARGO_TARGET_DIR:-$ROOT/platform/services/chorus-hooks/target}/debug/chorus-hook-shim"
  if [ ! -x "$SHIM_BIN" ]; then
    (cd "$ROOT/platform/services/chorus-hooks" && cargo build --offline --frozen --bin chorus-hook-shim)
  fi
  export CHORUS_HOOK_SHIM="$SHIM_BIN"

  # Arbitrary verb is refused
  run bash "$ROOT/platform/scripts/launchagent-ops" destroy com.chorus.api
  test "$status" -eq 2 || return 1
  [[ "$output" =~ "unknown verb" ]] || return 1

  # Extra arguments / flags are refused (helper takes label only)
  run bash "$ROOT/platform/scripts/launchagent-ops" kickstart com.chorus.api --force
  test "$status" -eq 2 || return 1
  [[ "$output" =~ "helper takes a label only" ]] || return 1

  # Plist path instead of label is refused
  run bash "$ROOT/platform/scripts/launchagent-ops" load "$T/com.chorus.api.plist"
  test "$status" -eq 2 || return 1
  [[ "$output" =~ "invalid label" ]] || return 1

  # Injection attempt in label is refused
  run bash "$ROOT/platform/scripts/launchagent-ops" kickstart "com.chorus.api;whoami"
  test "$status" -eq 2 || return 1
  [[ "$output" =~ "invalid label" ]] || return 1
}
