#!/usr/bin/env bats

# #2571 — chorus-env-setup.sh contract tests
#
# Retires shell-rc-discipline: sourcing the script from anywhere must
# resolve CHORUS_ROOT correctly via the script's own location, not from
# cwd or a pre-existing env.

setup() {
  SETUP="$BATS_TEST_DIRNAME/../chorus-env-setup.sh"
  [ -f "$SETUP" ] || skip "chorus-env-setup.sh not yet present"
}

@test "sourcing from arbitrary cwd resolves CHORUS_ROOT to the chorus checkout" {
  cd /tmp
  unset CHORUS_ROOT
  source "$SETUP"
  [ -n "$CHORUS_ROOT" ]
  [ -d "$CHORUS_ROOT/platform/scripts" ]
  [ -f "$CHORUS_ROOT/platform/scripts/chorus-env-setup.sh" ]
}

@test "subprocess invocation resolves CHORUS_ROOT correctly" {
  unset CHORUS_ROOT
  result=$(bash -c "source '$SETUP' && echo \$CHORUS_ROOT")
  [ -n "$result" ]
  [ -d "$result/platform/scripts" ]
}

@test "setup is idempotent — re-sourcing does not error or redefine" {
  source "$SETUP"
  first="$CHORUS_ROOT"
  source "$SETUP"
  [ "$CHORUS_ROOT" = "$first" ]
}

@test "setup ignores stale pre-set CHORUS_ROOT and resolves from script location" {
  export CHORUS_ROOT=/nonexistent/wrong/path
  source "$SETUP"
  [ "$CHORUS_ROOT" != "/nonexistent/wrong/path" ]
  [ -d "$CHORUS_ROOT/platform/scripts" ]
}

@test "CHORUS_ROLE still set when sourced from a roles/<name>/ cwd" {
  cd "$BATS_TEST_DIRNAME/../../../roles/silas"
  unset CHORUS_ROLE
  source "$SETUP"
  [ "$CHORUS_ROLE" = "silas" ]
}

@test "zsh-interactive: setup resolves CHORUS_ROOT correctly under zsh emulation" {
  # Jeff uses zsh interactively. BASH_SOURCE doesn't expand in zsh; the
  # setup's `${BASH_SOURCE[0]:-${(%):-%x}}` fallback to %x must work.
  command -v zsh >/dev/null || skip "zsh not available"
  result=$(zsh -c "unset CHORUS_ROOT; source '$SETUP' && echo \$CHORUS_ROOT")
  [ -n "$result" ]
  [ -d "$result/platform/scripts" ]
}

@test "per-worktree resolution: each worktree resolves to its own root" {
  # Simulated by checking the actual script-location-derived path matches
  # the absolute path of the chorus-env-setup.sh file we sourced.
  source "$SETUP"
  resolved="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd -P)"
  [ "$CHORUS_ROOT" = "$resolved" ]
}
