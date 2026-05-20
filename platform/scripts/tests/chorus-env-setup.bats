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

# --- #2735: werk + bin env vars ---

@test "CHORUS_WERK_BASE has a default" {
  unset CHORUS_WERK_BASE
  source "$SETUP"
  [ -n "$CHORUS_WERK_BASE" ]
}

@test "CHORUS_WERK_BASE caller-provided value is preserved" {
  export CHORUS_WERK_BASE=/custom/werk/base
  source "$SETUP"
  [ "$CHORUS_WERK_BASE" = "/custom/werk/base" ]
}

@test "CHORUS_BIN points at ~/.chorus/bin" {
  unset CHORUS_BIN
  source "$SETUP"
  [ "$CHORUS_BIN" = "$HOME/.chorus/bin" ]
}

@test "PATH is prepended with CHORUS_BIN exactly once on idempotent re-source" {
  unset CHORUS_BIN
  source "$SETUP"
  first_path="$PATH"
  # First source must put CHORUS_BIN ahead of everything
  [[ "$first_path" == "$CHORUS_BIN":* ]]
  source "$SETUP"
  # Second source must not duplicate it
  count=$(echo "$PATH" | tr ':' '\n' | grep -c "^$CHORUS_BIN$" || true)
  [ "$count" = "1" ]
}

@test "<ROLE>_WERK points at the single matching werk when exactly one exists" {
  # Ephemeral model (#2913): <ROLE>_WERK resolves to the role's single
  # <role>-<card> werk dir, not a persistent $CHORUS_WERK_BASE/<role> path.
  tmpbase="$(mktemp -d)"
  mkdir -p "$tmpbase/kade-1234"
  run bash -ec "cd /tmp; unset KADE_WERK; export CHORUS_ROLE=kade CHORUS_WERK_BASE='$tmpbase'; source '$SETUP'; echo \$KADE_WERK"
  rm -rf "$tmpbase"
  [ "$status" -eq 0 ]
  [ "$output" = "$tmpbase/kade-1234" ]
}

@test "<ROLE>_WERK unset when multiple werks exist (cannot disambiguate)" {
  # Two werks in flight → no single answer → leave the uppercase var unset
  # rather than guess (same discipline as resolveWorkingTree).
  tmpbase="$(mktemp -d)"
  mkdir -p "$tmpbase/wren-1" "$tmpbase/wren-2"
  run bash -ec "cd /tmp; unset WREN_WERK; export CHORUS_ROLE=wren CHORUS_WERK_BASE='$tmpbase'; source '$SETUP'; echo OK:\${WREN_WERK:-unset}"
  rm -rf "$tmpbase"
  [ "$status" -eq 0 ]
  [ "$output" = "OK:unset" ]
}

@test "no <ROLE>_WERK set when role can't be inferred" {
  cd /tmp
  unset CHORUS_ROLE KADE_WERK WREN_WERK SILAS_WERK
  source "$SETUP"
  # CHORUS_ROLE not set → no role-specific werk var
  [ -z "${KADE_WERK:-}" ]
  [ -z "${WREN_WERK:-}" ]
  [ -z "${SILAS_WERK:-}" ]
}

@test "set -e: role set with zero werks must not abort sourcing (grep -c on empty input)" {
  # Regression (#3012): `grep -c .` exits 1 on empty input. When a role had
  # zero <role>-* werks, the werk-count assignment failed; callers that run
  # `set -euo pipefail` (e.g. git-queue.sh) aborted silently before doing any
  # work — breaking commit/push for any role with no active werks. The clean
  # teardown of a role's last werk trips this directly.
  tmpbase="$(mktemp -d)"
  run bash -ec "cd /tmp; unset KADE_WERK; export CHORUS_ROLE=kade CHORUS_WERK_BASE='$tmpbase'; source '$SETUP'; echo OK:\${KADE_WERK:-unset}"
  rm -rf "$tmpbase"
  [ "$status" -eq 0 ]
  [ "$output" = "OK:unset" ]
}

@test "CHORUS_HOME is the canonical chorus checkout (sibling to chorus-werk)" {
  unset CHORUS_HOME
  source "$SETUP"
  [ -n "$CHORUS_HOME" ]
  # Should end with /chorus, not /chorus-werk/<role>
  [[ "$CHORUS_HOME" == */chorus ]]
  [[ "$CHORUS_HOME" != *chorus-werk* ]]
}

@test "CHORUS_HOME equals CHORUS_ROOT when sourced from canonical" {
  # Sourcing from canonical's own platform/scripts/ resolves CHORUS_ROOT
  # to canonical; CHORUS_HOME should match it.
  source "$SETUP"
  case "$CHORUS_ROOT" in
    *chorus-werk/*) skip "test runs from canonical only" ;;
    */chorus)
      [ "$CHORUS_HOME" = "$CHORUS_ROOT" ] ;;
  esac
}
