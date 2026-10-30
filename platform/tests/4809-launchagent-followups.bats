#!/usr/bin/env bats
# @test-type: unit
# #4809 — follow-ups from #4787's prod checks.
# Each AC runs its Rust/bash test from here so borg-prove sees one file per card.

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

@test "AC2: every launchagent-ops refusal emits launchagent.refused with role, label and reason" {
  cd "$ROOT/platform/services/chorus-hooks"
  run cargo test --offline --bin chorus-hooks launchagent_ops_tests::refusal_emits_launchagent_refused_with_role_label_reason
  echo "$output"
  [ "$status" -eq 0 ]
}
