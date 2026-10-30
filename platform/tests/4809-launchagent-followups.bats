#!/usr/bin/env bats
# @test-type: unit
# #4809 — follow-ups from #4787's prod checks.
# Each AC runs its Rust/bash test from here so borg-prove sees one file per card.

bats_require_minimum_version 1.5.0

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export ROOT
  # Caller's CARGO_TARGET_DIR wins. Otherwise reuse the crate's warm target/,
  # unless this account can't write it (#4787: pipeline account vs role-owned
  # target → PermissionDenied), then fall back to a cold per-file target.
  if [ -z "${CARGO_TARGET_DIR:-}" ]; then
    local crate_target="$ROOT/platform/services/chorus-hooks/target"
    # -w on the dir is not enough: files inside may belong to another account.
    if [ -d "$crate_target" ] && [ -w "$crate_target" ] \
       && [ -z "$(find "$crate_target" -maxdepth 3 ! -user "$(id -un)" -print -quit 2>/dev/null)" ]; then
      export CARGO_TARGET_DIR="$crate_target"
    else
      export CARGO_TARGET_DIR="$BATS_FILE_TMPDIR/cargo-target"
    fi
  fi
}

@test "AC1: launchagent-ops refusals exit non-zero, so a script can tell it was refused" {
  cd "$ROOT/platform/services/chorus-hooks"
  run cargo build --bin chorus-hook-shim
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  SHIM="$CARGO_TARGET_DIR/debug/chorus-hook-shim"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  run "$SHIM" launchagent-ops kickstart org.other.service
  [ "$status" -ne 0 ] || { echo "invalid label exited 0"; return 1; }
  run "$SHIM" launchagent-ops destroy com.chorus.clearing
  [ "$status" -ne 0 ] || { echo "unknown verb exited 0"; return 1; }
}

@test "AC2 (CLI): refusals the shim makes before reaching the daemon also emit launchagent.refused" {
  cd "$ROOT/platform/services/chorus-hooks"
  run cargo build --bin chorus-hook-shim
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  SHIM="$CARGO_TARGET_DIR/debug/chorus-hook-shim"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  : > "$CHORUS_LOG_FILE"

  run "$SHIM" launchagent-ops kickstart org.other.service
  [ "$status" -ne 0 ]
  run "$SHIM" launchagent-ops destroy com.chorus.clearing
  [ "$status" -ne 0 ]
  run "$SHIM" launchagent-ops kickstart com.chorus.clearing /tmp/evil.plist
  [ "$status" -ne 0 ]

  for reason in invalid-label unknown-verb extra-args; do
    grep '"event":"launchagent.refused"' "$CHORUS_LOG_FILE" | grep -q "$reason" \
      || { echo "no launchagent.refused with reason=$reason:"; cat "$CHORUS_LOG_FILE"; return 1; }
  done
}

@test "AC2: every launchagent-ops refusal emits launchagent.refused with role, label and reason" {
  cd "$ROOT/platform/services/chorus-hooks"
  run cargo test --bin chorus-hooks launchagent_ops_tests::refusal_emits_launchagent_refused_with_role_label_reason
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC3a: an abnormal previous run killed by a signal reports exit_code=128+signo" {
  . "$ROOT/platform/scripts/lib/service-lifecycle.sh"
  print_out="$(printf '\tlast terminating signal = Killed: 9\n\tlast exit code = (never exited)\n')"
  out="$(_sl_previous_end "$print_out")"
  [[ "$out" == *"signal=Killed: 9"* ]] || { echo "no signal: $out"; return 1; }
  [[ "$out" == *"exit_code=137"* ]] || { echo "no exit_code=137: $out"; return 1; }
}

@test "AC3b: a missing program reports exit_code=127, the same field name as every other failure" {
  export CHORUS_HOME="$ROOT"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  : > "$CHORUS_LOG_FILE"
  run -127 bash "$ROOT/platform/scripts/service-run" com.chorus.probe-4809 job /nonexistent/program-4809
  [ "$status" -eq 127 ]
  line="$(grep '"event":"service.failed"' "$CHORUS_LOG_FILE" | tail -1)"
  [[ "$line" == *'"exit_code":"127"'* || "$line" == *'"exit_code":127'* ]] || { echo "no exit_code=127: $line"; return 1; }
  [[ "$line" != *'"exit":'* ]] || { echo "old exit= field still present: $line"; return 1; }
}

@test "AC4: no test-only env switch in prod service-run or its lifecycle lib" {
  run grep -rn '_SERVICE_RUN_TEST' "$ROOT/platform/scripts/service-run" "$ROOT/platform/scripts/lib/service-lifecycle.sh"
  [ "$status" -eq 1 ] || { echo "test-only switch still in prod: $output"; return 1; }
}
